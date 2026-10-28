//! Original multiscale PatchMatch synthesis. No browser, GPU, or reference code.
use crate::fill_sample;
use fotoeditor_core::*;
use std::sync::Arc;

const R: i32 = 3;
const ITERATIONS: usize = 6;
const TILE_BYTES: u64 = (TILE_PIXELS * 4) as u64;

#[derive(Clone, Copy, Default)]
struct Match {
    x: u16,
    y: u16,
    score: f32,
}
struct Level {
    w: usize,
    h: usize,
    source: Vec<Rgba8>,
    mask: Vec<u8>,
    target: Vec<Rgba8>,
    next: Vec<Rgba8>,
    field: Vec<Match>,
    valid: Vec<bool>,
    donors: Vec<u32>,
}
impl Level {
    fn new(w: usize, h: usize) -> Self {
        let n = w * h;
        Self {
            w,
            h,
            source: Vec::with_capacity(n),
            mask: Vec::with_capacity(n),
            target: Vec::with_capacity(n),
            next: Vec::with_capacity(n),
            field: Vec::with_capacity(n),
            valid: Vec::with_capacity(n),
            donors: Vec::with_capacity(n),
        }
    }
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.w as i32 && y < self.h as i32)
            .then(|| y as usize * self.w + x as usize)
    }
    fn donor(&self, x: i32, y: i32) -> Option<usize> {
        self.index(x, y).filter(|i| self.valid[*i])
    }
    fn distance(&self, i: usize, x: i32, y: i32) -> f32 {
        let tx = (i % self.w) as i32;
        let ty = (i / self.w) as i32;
        let mut error = 0.0;
        let mut weight = 0.0;
        for dy in -R..=R {
            for dx in -R..=R {
                let Some(t) = self.index(tx + dx, ty + dy) else {
                    continue;
                };
                let a = self.target[t].0;
                // Transparent target RGB is never evidence. Donors are opaque.
                if a[3] == 0 {
                    continue;
                }
                let b = self.source[(y + dy) as usize * self.w + (x + dx) as usize].0;
                let w = if self.mask[t] == 0 { 1.0 } else { 0.15 };
                let alpha = f32::from(a[3]) / 255.0;
                for c in 0..3 {
                    error += w * (f32::from(a[c]) * alpha - f32::from(b[c])).powi(2);
                }
                error += w * (f32::from(a[3]) - 255.0).powi(2);
                weight += w;
            }
        }
        if weight == 0.0 {
            0.0
        } else {
            error / weight
        }
    }
    fn improve(&self, i: usize, best: &mut Match, x: i32, y: i32) {
        if self.donor(x, y).is_none() {
            return;
        }
        let score = self.distance(i, x, y);
        // Fixed traversal and strict improvement keep repeated fills reproducible.
        if score < best.score {
            *best = Match {
                x: x as u16,
                y: y as u16,
                score,
            };
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Scan,
    Capture,
    Pyramid,
    Valid,
    NearestForward,
    NearestBackward,
    Seed,
    Match,
    Vote,
    Write,
    Done,
}

/// Immutable source, private candidate and bounded cooperative processing stages.
/// `available` is additional shared CPU residency, after document/history/source.
pub struct ContentAwareJob {
    pub before: Document,
    pub candidate: Document,
    source: Arc<TileSurface<Rgba8>>,
    layer: LayerId,
    stage: Stage,
    cursor: usize,
    bounds: [u32; 4],
    roi: [u32; 4],
    levels: Vec<Level>,
    level: usize,
    iteration: usize,
    rng: u32,
    available: u64,
    reserve: u64,
    alpha_lock: bool,
    // Exposed diagnostics describe executed algorithm work, not timing claims.
    pub propagation_trials: u64,
    pub random_trials: u64,
}
impl ContentAwareJob {
    pub fn new(
        document: &Document,
        layer: LayerId,
        source: Arc<TileSurface<Rgba8>>,
        available: u64,
    ) -> Result<Self, String> {
        check_layer_edit(&document.layers, layer, false)?;
        document.layer(layer)?.raster_samples()?;
        document
            .selection
            .as_ref()
            .ok_or("Select an area for content-aware fill first")?;
        if (source.width, source.height) != (document.width, document.height) {
            return Err("Content-aware source dimensions differ".into());
        }
        if available < 3 * TILE_BYTES {
            return Err("Content-aware fill exceeds the shared CPU budget".into());
        }
        Ok(Self {
            before: document.clone(),
            candidate: document.clone(),
            source,
            layer,
            stage: Stage::Scan,
            cursor: 0,
            bounds: [document.width, document.height, 0, 0],
            roi: [0; 4],
            levels: Vec::new(),
            level: 0,
            iteration: 0,
            rng: 0x4347_5039,
            available,
            reserve: 2 * TILE_BYTES,
            alpha_lock: document.layer(layer)?.properties.locks.alpha,
            propagation_trials: 0,
            random_trials: 0,
        })
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.reserve
    }
    pub fn visit_candidates(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.before.visit_tiles(v);
        self.candidate.visit_tiles(v);
        for t in self.source.tiles().values() {
            v(TileRef::Rgba(t));
        }
    }
    fn tile_coord(&self) -> TileCoord {
        let cols = self.before.width.div_ceil(TILE_SIZE) as usize;
        ((self.cursor % cols) as u32, (self.cursor / cols) as u32)
    }
    /// Only scan/capture/write need authoritative tiles. Dense algorithm stages
    /// use admitted immutable samples and never pin the entire document.
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if self.stage == Stage::Write && !self.write_tile_intersects() {
            return;
        }
        let c = match self.stage {
            Stage::Scan | Stage::Write => self.tile_coord(),
            Stage::Capture => {
                let w = (self.roi[2] - self.roi[0]) as usize;
                (
                    (self.roi[0] + (self.cursor % w) as u32) / TILE_SIZE,
                    (self.roi[1] + (self.cursor / w) as u32) / TILE_SIZE,
                )
            }
            _ => return,
        };
        if let Some(t) = self
            .before
            .selection
            .as_ref()
            .and_then(|s| s.tiles().get(&c))
        {
            v(TileRef::Gray(t));
        }
        if self.stage == Stage::Capture {
            if let Some(t) = self.source.tiles().get(&c) {
                v(TileRef::Rgba(t));
            }
        }
        if self.stage == Stage::Write {
            if let Some(t) = self
                .before
                .layer(self.layer)
                .unwrap()
                .raster_samples()
                .unwrap()
                .tiles()
                .get(&c)
            {
                v(TileRef::Rgba(t));
            }
        }
    }
    fn next(&mut self, stage: Stage) {
        self.stage = stage;
        self.cursor = 0;
    }
    fn random(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % n
    }
    fn scan(&mut self) -> Result<(), String> {
        let c = self.tile_coord();
        let selection = self.before.selection.as_ref().unwrap();
        let tile = selection.tiles().get(&c).map(|t| t.pin()).transpose()?;
        let canvas = self.before.canvas_rect();
        for y in c.1 * TILE_SIZE..((c.1 + 1) * TILE_SIZE).min(self.before.height) {
            for x in c.0 * TILE_SIZE..((c.0 + 1) * TILE_SIZE).min(self.before.width) {
                let a = tile.as_ref().map_or(selection.default.0, |t| {
                    t.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize].0
                });
                if a > 0 && canvas.contains(x, y) {
                    self.bounds[0] = self.bounds[0].min(x);
                    self.bounds[1] = self.bounds[1].min(y);
                    self.bounds[2] = self.bounds[2].max(x + 1);
                    self.bounds[3] = self.bounds[3].max(y + 1);
                }
            }
        }
        self.cursor += 1;
        if self.cursor
            == (self.before.width.div_ceil(TILE_SIZE) * self.before.height.div_ceil(TILE_SIZE))
                as usize
        {
            if self.bounds[0] >= self.bounds[2] {
                return Err("Select a nonempty area inside the visible canvas".into());
            }
            // Deterministic neighborhood: 128..512 px beyond the selected bbox.
            let margin = (self.bounds[2] - self.bounds[0])
                .max(self.bounds[3] - self.bounds[1])
                .clamp(128, 512);
            self.roi = [
                self.bounds[0].saturating_sub(margin).max(canvas.x),
                self.bounds[1].saturating_sub(margin).max(canvas.y),
                self.bounds[2]
                    .saturating_add(margin)
                    .min(canvas.x + canvas.width),
                self.bounds[3]
                    .saturating_add(margin)
                    .min(canvas.y + canvas.height),
            ];
            let mut w = (self.roi[2] - self.roi[0]) as usize;
            let mut h = (self.roi[3] - self.roi[1]) as usize;
            let mut selected =
                (self.bounds[2] - self.bounds[0]).max(self.bounds[3] - self.bounds[1]);
            let mut sizes = vec![(w, h)];
            while selected > 32 && w >= 32 && h >= 32 && sizes.len() < 6 {
                w = w.div_ceil(2);
                h = h.div_ceil(2);
                selected = selected.div_ceil(2);
                sizes.push((w, h));
            }
            // 4 source + 1 mask + 4 target + 4 next + 8 field + 1 valid +
            // 4 donor indices = 26 bytes/pixel; use the actual Match layout.
            self.reserve = sizes
                .iter()
                .map(|(w, h)| (w * h) as u64 * (18 + size_of::<Match>() as u64))
                .sum::<u64>()
                + 2 * TILE_BYTES;
            let output_tiles =
                (self.bounds[2] - 1).div_euclid(TILE_SIZE) - self.bounds[0] / TILE_SIZE + 1;
            let output_rows =
                (self.bounds[3] - 1).div_euclid(TILE_SIZE) - self.bounds[1] / TILE_SIZE + 1;
            if self.reserve + u64::from(output_tiles) * u64::from(output_rows) * TILE_BYTES
                > self.available
            {
                return Err(
                    "Content-aware fill exceeds the shared CPU budget; document unchanged".into(),
                );
            }
            self.levels = sizes.into_iter().map(|(w, h)| Level::new(w, h)).collect();
            self.next(Stage::Capture);
        }
        Ok(())
    }
    fn capture(&mut self) -> Result<(), String> {
        let l = &mut self.levels[0];
        let x = self.roi[0] + (self.cursor % l.w) as u32;
        let y = self.roi[1] + (self.cursor / l.w) as u32;
        let c = (x / TILE_SIZE, y / TILE_SIZE);
        let source = self.source.tiles().get(&c).map(|t| t.pin()).transpose()?;
        let selection = self.before.selection.as_ref().unwrap();
        let mask = selection.tiles().get(&c).map(|t| t.pin()).transpose()?;
        let count = (TILE_SIZE - x % TILE_SIZE).min(self.roi[2] - x) as usize;
        for dx in 0..count {
            let i = ((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize + dx;
            l.source
                .push(source.as_ref().map_or(self.source.default, |t| t.pixels[i]));
            l.mask
                .push(mask.as_ref().map_or(selection.default.0, |t| t.pixels[i].0));
        }
        self.cursor += count;
        if self.cursor == l.w * l.h {
            if self.levels.len() > 1 {
                self.level = 1;
                self.next(Stage::Pyramid);
            } else {
                self.next(Stage::Valid);
            }
        }
        Ok(())
    }
    fn pyramid(&mut self) {
        let (parents, children) = self.levels.split_at_mut(self.level);
        let p = &parents[self.level - 1];
        let l = &mut children[0];
        let end = (self.cursor + 2048).min(l.w * l.h);
        for i in self.cursor..end {
            let mut sums = [0u32; 4];
            let mut count = 0;
            let mut mask = 0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let x = (i % l.w) * 2 + dx;
                    let y = (i / l.w) * 2 + dy;
                    if x >= p.w || y >= p.h {
                        continue;
                    }
                    let n = y * p.w + x;
                    mask = mask.max(p.mask[n]);
                    // Selected samples never seed synthesis, even in coarse levels.
                    if p.mask[n] == 0 && p.source[n].0[3] == 255 {
                        for (sum, c) in sums.iter_mut().zip(p.source[n].0) {
                            *sum += u32::from(c);
                        }
                        count += 1;
                    }
                }
            }
            l.source.push(if count == 0 {
                Rgba8::default()
            } else {
                Rgba8(sums.map(|s| ((s + count / 2) / count) as u8))
            });
            // A coarse pixel is donor-safe only if every constituent is opaque.
            if mask == 0 {
                let all_opaque = (0..2).all(|dy| {
                    (0..2).all(|dx| {
                        let x = (i % l.w) * 2 + dx;
                        let y = (i / l.w) * 2 + dy;
                        x >= p.w || y >= p.h || p.source[y * p.w + x].0[3] == 255
                    })
                });
                if !all_opaque {
                    l.source[i].0[3] = 0;
                }
            }
            l.mask.push(mask);
        }
        self.cursor = end;
        if end == l.w * l.h {
            if self.level + 1 < self.levels.len() {
                self.level += 1;
                self.cursor = 0;
            } else {
                self.next(Stage::Valid);
            }
        }
    }
    fn valid(&mut self) -> Result<(), String> {
        let l = &mut self.levels[self.level];
        let end = (self.cursor + 512).min(l.w * l.h);
        for i in self.cursor..end {
            let x = (i % l.w) as i32;
            let y = (i / l.w) as i32;
            let valid = x >= R
                && y >= R
                && x + R < l.w as i32
                && y + R < l.h as i32
                && (-R..=R).all(|dy| {
                    (-R..=R).all(|dx| {
                        let j = (y + dy) as usize * l.w + (x + dx) as usize;
                        l.mask[j] == 0 && l.source[j].0[3] == 255
                    })
                });
            l.valid.push(valid);
            if valid {
                l.donors.push(i as u32);
            }
            l.field.push(Match {
                x: x as u16,
                y: y as u16,
                score: if valid { 0.0 } else { f32::INFINITY },
            });
            l.target.push(l.source[i]);
            l.next.push(l.source[i]);
        }
        self.cursor = end;
        if end == l.w * l.h {
            if l.donors.is_empty() {
                if self.level == 0 {
                    return Err("No suitable opaque donor patch outside the selection; select a smaller area or sample all layers".into());
                }
                self.levels.pop();
                self.level -= 1;
                self.next(Stage::Valid);
            } else {
                self.next(Stage::NearestForward);
            }
        }
        Ok(())
    }
    fn nearest(&mut self, reverse: bool) {
        let l = &mut self.levels[self.level];
        let n = l.w * l.h;
        let end = (self.cursor + 2048).min(n);
        for at in self.cursor..end {
            let i = if reverse { n - 1 - at } else { at };
            let x = (i % l.w) as i32;
            let y = (i / l.w) as i32;
            for (dx, dy) in if reverse {
                [(1, 0), (0, 1)]
            } else {
                [(-1, 0), (0, -1)]
            } {
                if let Some(j) = l.index(x + dx, y + dy) {
                    let m = l.field[j];
                    if !m.score.is_finite() {
                        continue;
                    }
                    let d = ((x - i32::from(m.x)).pow(2) + (y - i32::from(m.y)).pow(2)) as f32;
                    if d < l.field[i].score {
                        l.field[i] = Match { score: d, ..m };
                    }
                }
            }
        }
        self.cursor = end;
        if end == n {
            self.next(if reverse {
                Stage::Seed
            } else {
                Stage::NearestBackward
            });
        }
    }
    fn seed(&mut self) {
        let (fine, coarse) = self.levels.split_at_mut(self.level + 1);
        let l = &mut fine[self.level];
        let parent = coarse.first();
        let end = (self.cursor + 2048).min(l.w * l.h);
        for i in self.cursor..end {
            if l.mask[i] != 0 {
                let m = l.field[i];
                l.target[i] = parent.map_or(l.source[m.y as usize * l.w + m.x as usize], |p| {
                    p.target[(i / l.w / 2) * p.w + i % l.w / 2]
                });
            }
        }
        self.cursor = end;
        if end == l.w * l.h {
            self.iteration = 0;
            self.next(Stage::Match);
        }
    }
    fn matching(&mut self) {
        let count = self.levels[self.level].w * self.levels[self.level].h;
        let end = (self.cursor + 32).min(count);
        let reverse = self.iteration % 2 == 1;
        for at in self.cursor..end {
            let i = if reverse { count - 1 - at } else { at };
            if self.levels[self.level].mask[i] == 0 {
                continue;
            }
            let l = &self.levels[self.level];
            let mut best = l.field[i];
            best.score = l.distance(i, i32::from(best.x), i32::from(best.y));
            let x = (i % l.w) as i32;
            let y = (i / l.w) as i32;
            for (dx, dy) in if reverse {
                [(1, 0), (0, 1)]
            } else {
                [(-1, 0), (0, -1)]
            } {
                if let Some(j) = l.index(x + dx, y + dy) {
                    let m = l.field[j];
                    l.improve(i, &mut best, i32::from(m.x) - dx, i32::from(m.y) - dy);
                    self.propagation_trials += 1;
                }
            }
            // Random donor initialization also escapes invalid sparse searches.
            let n = self.levels[self.level].donors.len() as u32;
            let random = self.random(n) as usize;
            let l = &self.levels[self.level];
            let d = l.donors[random] as usize;
            l.improve(i, &mut best, (d % l.w) as i32, (d / l.w) as i32);
            self.random_trials += 1;
            let mut radius = l.w.max(l.h) as i32;
            let origin = best;
            while radius > 0 {
                let dx = self.random((2 * radius + 1) as u32) as i32 - radius;
                let dy = self.random((2 * radius + 1) as u32) as i32 - radius;
                self.levels[self.level].improve(
                    i,
                    &mut best,
                    i32::from(origin.x) + dx,
                    i32::from(origin.y) + dy,
                );
                self.random_trials += 1;
                radius /= 2;
            }
            self.levels[self.level].field[i] = best;
        }
        self.cursor = end;
        if end == count {
            self.next(Stage::Vote);
        }
    }
    fn vote(&mut self) {
        let l = &mut self.levels[self.level];
        let end = (self.cursor + 256).min(l.w * l.h);
        for i in self.cursor..end {
            if l.mask[i] == 0 {
                continue;
            }
            let x = (i % l.w) as i32;
            let y = (i / l.w) as i32;
            let mut sum = [0.0f64; 3];
            let mut total = 0.0;
            for dy in -R..=R {
                for dx in -R..=R {
                    let Some(j) = l.index(x + dx, y + dy) else {
                        continue;
                    };
                    // Known centers vote only when their identity is donor-safe.
                    if l.mask[j] == 0 && !l.valid[j] {
                        continue;
                    }
                    let m = l.field[j];
                    let sx = i32::from(m.x) - dx;
                    let sy = i32::from(m.y) - dy;
                    let Some(s) = l.index(sx, sy) else { continue };
                    let w = 1.0 / (1.0 + f64::from(m.score) / 256.0);
                    for (out, c) in sum.iter_mut().zip(l.source[s].0) {
                        *out += f64::from(c) * w;
                    }
                    total += w;
                }
            }
            if total > 0.0 {
                let rgb = sum.map(|s| (s / total + 0.5) as u8);
                l.next[i] = Rgba8([rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        self.cursor = end;
        if end == l.w * l.h {
            std::mem::swap(&mut l.target, &mut l.next);
            self.iteration += 1;
            if self.iteration < ITERATIONS {
                self.next(Stage::Match);
            } else if self.level > 0 {
                self.level -= 1;
                self.next(Stage::Valid);
            } else {
                self.next(Stage::Write);
            }
        }
    }
    fn write(&mut self) -> Result<(), String> {
        if !self.write_tile_intersects() {
            self.advance_write();
            return Ok(());
        }
        let c = self.tile_coord();
        let l = &self.levels[0];
        let source = self.before.layer(self.layer)?.raster_samples()?;
        let old = source.tiles().get(&c).map(|t| t.pin()).transpose()?;
        let mut out = old
            .as_ref()
            .map_or_else(|| vec![source.default; TILE_PIXELS], |t| t.pixels.clone());
        let mut changed = false;
        let x0 = (c.0 * TILE_SIZE).max(self.bounds[0]);
        let x1 = ((c.0 + 1) * TILE_SIZE).min(self.bounds[2]);
        let y0 = (c.1 * TILE_SIZE).max(self.bounds[1]);
        let y1 = ((c.1 + 1) * TILE_SIZE).min(self.bounds[3]);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y - self.roi[1]) as usize) * l.w + (x - self.roi[0]) as usize;
                let at = ((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize;
                let after = fill_sample(out[at], l.target[i].0, l.mask[i], false, self.alpha_lock);
                changed |= after != out[at];
                out[at] = after;
            }
        }
        if changed {
            self.candidate
                .layer_mut(self.layer)?
                .raster_samples_mut()?
                .insert(c, out);
        }
        self.advance_write();
        Ok(())
    }
    fn write_tile_intersects(&self) -> bool {
        let c = self.tile_coord();
        c.0 * TILE_SIZE < self.bounds[2]
            && (c.0 + 1) * TILE_SIZE > self.bounds[0]
            && c.1 * TILE_SIZE < self.bounds[3]
            && (c.1 + 1) * TILE_SIZE > self.bounds[1]
    }
    fn advance_write(&mut self) {
        self.cursor += 1;
        if self.cursor
            == (self.before.width.div_ceil(TILE_SIZE) * self.before.height.div_ceil(TILE_SIZE))
                as usize
        {
            self.next(Stage::Done);
        }
    }
    pub fn step(&mut self) -> Result<bool, String> {
        match self.stage {
            Stage::Scan => self.scan()?,
            Stage::Capture => self.capture()?,
            Stage::Pyramid => self.pyramid(),
            Stage::Valid => self.valid()?,
            Stage::NearestForward => self.nearest(false),
            Stage::NearestBackward => self.nearest(true),
            Stage::Seed => self.seed(),
            Stage::Match => self.matching(),
            Stage::Vote => self.vote(),
            Stage::Write => self.write()?,
            Stage::Done => {}
        }
        Ok(self.stage == Stage::Done)
    }
    pub fn finish(self) -> Result<TileCommand, String> {
        if self.stage != Stage::Done {
            return Err("Content-aware fill is incomplete".into());
        }
        Ok(
            TileCommand::for_layer(&self.before, &self.candidate, self.layer)?
                .named("Content-aware fill"),
        )
    }
}

#[cfg(test)]
mod tests;
