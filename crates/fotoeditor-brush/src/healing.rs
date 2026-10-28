//! Original, browser-independent local patch search and Dirichlet Poisson solve.
//! No candidate pixels are exposed until the complete dab has converged.
use super::*;
use fotoeditor_core::{TileRef, TileSurface};

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HealingOptions {
    pub spot: bool,
    pub aligned: bool,
    pub all_layers: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Search,
    Gather,
    Domain,
    Initialize,
    Multiply,
    Update,
    Direction,
    Verify,
    Output,
    Done,
}

pub(super) struct HealingPatch {
    center: [f64; 2],
    radius: f64,
    x0: i32,
    y0: i32,
    width: usize,
    height: usize,
    source: Vec<Rgba8>,
    target: Vec<Rgba8>,
    domain: Vec<u8>,
    solution: Vec<[f64; 3]>,
    residual: Vec<[f64; 3]>,
    direction: Vec<[f64; 3]>,
    product: Vec<[f64; 3]>,
    offset: [i32; 2],
    candidates: Vec<[i32; 2]>,
    search_index: usize,
    refine: i32,
    best: Option<([i32; 2], f64)>,
    stage: Stage,
    position: usize,
    rho: f64,
    sum: f64,
    maximum: f64,
    alpha: f64,
    beta: f64,
    iterations: usize,
}

impl HealingPatch {
    pub fn new(dab: &Dab, spot: bool, offset: [i32; 2], available: u64) -> Result<Self, String> {
        let width = (dab.x1 - dab.x0 + 2) as usize;
        let height = (dab.y1 - dab.y + 2) as usize;
        let n = width * height;
        // Four f64 RGB vectors, two RGBA8 surfaces and one byte domain. Reserve
        // the complete allocation before touching it, plus bounded search storage.
        let bytes = n as u64 * 105 + 192 * 8;
        if bytes > available {
            return Err(
                "Healing exceeds the document/history/temporary memory budget; stroke cancelled"
                    .into(),
            );
        }
        let mut candidates = Vec::with_capacity(192);
        if spot {
            for distance in [
                2.25 * dab.radius + 4.0,
                3.0 * dab.radius + 6.0,
                4.0 * dab.radius + 8.0,
            ] {
                for angle in 0..16 {
                    let a = f64::from(angle) * std::f64::consts::TAU / 16.0;
                    candidates.push([
                        (distance * a.cos()).round() as i32,
                        (distance * a.sin()).round() as i32,
                    ]);
                }
            }
        }
        Ok(Self {
            center: [dab.p.x, dab.p.y],
            radius: dab.radius,
            x0: dab.x0 as i32 - 1,
            y0: dab.y as i32 - 1,
            width,
            height,
            source: vec![Rgba8::default(); n],
            target: vec![Rgba8::default(); n],
            domain: vec![0; n],
            solution: vec![[0.0; 3]; n],
            residual: vec![[0.0; 3]; n],
            direction: vec![[0.0; 3]; n],
            product: vec![[0.0; 3]; n],
            offset,
            candidates,
            search_index: 0,
            refine: (dab.radius / 4.0).round().max(1.0) as i32,
            best: None,
            stage: if spot { Stage::Search } else { Stage::Gather },
            position: 0,
            rho: 0.0,
            sum: 0.0,
            maximum: 0.0,
            alpha: 0.0,
            beta: 0.0,
            iterations: 0,
        })
    }
    pub fn bytes(&self) -> u64 {
        ((self.source.capacity() + self.target.capacity()) * 4
            + self.domain.capacity()
            + (self.solution.capacity()
                + self.residual.capacity()
                + self.direction.capacity()
                + self.product.capacity())
                * 24
            + self.candidates.capacity() * 8) as u64
    }
    pub fn done(&self) -> bool {
        self.stage == Stage::Done
    }
    pub fn matches(&self, point: [f64; 2], radius: f64) -> bool {
        self.done() && self.center == point && self.radius == radius
    }
    pub fn work(&self) -> usize {
        if self.stage == Stage::Search {
            192
        } else {
            (self.source.len() - self.position).min(8192)
        }
    }
    pub fn sample(&self, x: u32, y: u32) -> Rgba8 {
        let i = (y as i32 - self.y0) as usize * self.width + (x as i32 - self.x0) as usize;
        if self.domain[i] == 0 {
            Rgba8::default()
        } else {
            self.source[i]
        }
    }
    pub fn needed(&self, doc: &Document, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if !matches!(self.stage, Stage::Search | Stage::Gather) {
            return;
        }
        let offset = if self.stage == Stage::Search {
            self.candidates[self.search_index]
        } else {
            self.offset
        };
        // Extra three pixels contain every search-ring probe as well as the
        // solver halo. Only the next candidate is hydrated, never the search box.
        let bounds = doc.canvas_rect();
        for [dx, dy] in [[0, 0], offset] {
            let x0 = (self.x0 + dx - 3).max(bounds.x as i32);
            let y0 = (self.y0 + dy - 3).max(bounds.y as i32);
            let x1 = (self.x0 + dx + self.width as i32 + 3).min((bounds.x + bounds.width) as i32);
            let y1 = (self.y0 + dy + self.height as i32 + 3).min((bounds.y + bounds.height) as i32);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            for y in y0 as u32 / TILE_SIZE..=(y1 as u32 - 1) / TILE_SIZE {
                for x in x0 as u32 / TILE_SIZE..=(x1 as u32 - 1) / TILE_SIZE {
                    if let Some(t) = doc.raster().tiles().get(&(x, y)) {
                        visitor(TileRef::Rgba(t));
                    }
                }
            }
        }
    }
    fn next(&mut self, stage: Stage) {
        self.stage = stage;
        self.position = 0;
        self.sum = 0.0;
        self.maximum = 0.0;
    }
    fn neighbors(&self, i: usize) -> [usize; 4] {
        [i - 1, i + 1, i - self.width, i + self.width]
    }
    fn search(&mut self, doc: &Document, dab: &Dab) -> Result<(), String> {
        let offset = self.candidates[self.search_index];
        let separated = f64::from(offset[0]).hypot(f64::from(offset[1])) > 2.0 * dab.radius + 3.0;
        if separated {
            let mut score = 0.0;
            for k in 0..48 {
                let angle = f64::from(k) * std::f64::consts::TAU / 48.0;
                let x = (dab.p.x + (dab.radius + 2.0) * angle.cos()).floor() as i32;
                let y = (dab.p.y + (dab.radius + 2.0) * angle.sin()).floor() as i32;
                let a = read(doc, x, y)?;
                let b = read(doc, x + offset[0], y + offset[1])?;
                if a.0[3] == 0 || b.0[3] == 0 {
                    score = f64::INFINITY;
                    break;
                }
                for c in 0..3 {
                    score += (f64::from(a.0[c]) - f64::from(b.0[c])).powi(2);
                }
            }
            if score.is_finite() && self.best.is_none_or(|(_, best)| score < best) {
                self.best = Some((offset, score));
            }
        }
        self.search_index += 1;
        if self.search_index == self.candidates.len() {
            let (best, _) = self.best.ok_or("No suitable nearby visible patch; choose another area or a smaller Spot Healing brush")?;
            if self.refine > 0 {
                for y in -2..=2 {
                    for x in -2..=2 {
                        self.candidates
                            .push([best[0] + x * self.refine, best[1] + y * self.refine]);
                    }
                }
                self.refine = if self.refine == 1 {
                    0
                } else {
                    (self.refine / 4).max(1)
                };
            } else {
                self.offset = best;
                self.next(Stage::Gather);
            }
        }
        Ok(())
    }
    /// Each call processes at most 8192 vector elements or 48 ring comparisons.
    /// Dot products are accumulated across chunks in a deterministic order.
    pub fn step(
        &mut self,
        doc: &Document,
        dab: &Dab,
        tip: &Tip,
        limit: usize,
    ) -> Result<(), String> {
        if self.stage == Stage::Search {
            return self.search(doc, dab);
        }
        if self.done() {
            return Ok(());
        }
        let n = self.source.len();
        let end = (self.position + limit.clamp(1, 8192)).min(n);
        for i in self.position..end {
            if self.stage == Stage::Gather {
                let x = self.x0 + (i % self.width) as i32;
                let y = self.y0 + (i / self.width) as i32;
                self.target[i] = read(doc, x, y)?;
                self.source[i] = read(doc, x + self.offset[0], y + self.offset[1])?;
                self.solution[i] = std::array::from_fn(|c| f64::from(self.target[i].0[c]));
                continue;
            }
            if self.stage == Stage::Domain {
                if i % self.width == 0
                    || i % self.width == self.width - 1
                    || i < self.width
                    || i >= n - self.width
                {
                    continue;
                }
                let scale = tip.radius / dab.radius;
                let tx = (f64::from(self.x0) + (i % self.width) as f64 + 0.5 - dab.p.x) * scale
                    + tip.radius
                    + 0.5;
                let ty = (f64::from(self.y0) + (i / self.width) as f64 + 0.5 - dab.p.y) * scale
                    + tip.radius
                    + 0.5;
                if tip.sample(tx, ty) == 0 {
                    continue;
                }
                if std::iter::once(i)
                    .chain(self.neighbors(i))
                    .all(|j| self.source[j].0[3] > 0 && self.target[j].0[3] > 0)
                {
                    self.domain[i] = 1;
                }
                continue;
            }
            if self.stage == Stage::Output {
                if self.domain[i] != 0 {
                    for c in 0..3 {
                        self.source[i].0[c] = self.solution[i][c].round().clamp(0.0, 255.0) as u8;
                    }
                }
                continue;
            }
            if self.domain[i] == 0 {
                continue;
            }
            match self.stage {
                Stage::Initialize | Stage::Verify => {
                    let neighbors = self.neighbors(i);
                    for c in 0..3 {
                        let r = 4.0 * (f64::from(self.source[i].0[c]) - self.solution[i][c])
                            - neighbors
                                .iter()
                                .map(|&j| f64::from(self.source[j].0[c]) - self.solution[j][c])
                                .sum::<f64>();
                        self.residual[i][c] = r;
                        self.direction[i][c] = r;
                        self.sum += r * r;
                        self.maximum = self.maximum.max(r.abs());
                    }
                }
                Stage::Multiply => {
                    for c in 0..3 {
                        let ap = 4.0 * self.direction[i][c]
                            - self
                                .neighbors(i)
                                .iter()
                                .map(|&j| self.direction[j][c])
                                .sum::<f64>();
                        self.product[i][c] = ap;
                        self.sum += self.direction[i][c] * ap;
                    }
                }
                Stage::Update => {
                    for c in 0..3 {
                        self.solution[i][c] += self.alpha * self.direction[i][c];
                        self.residual[i][c] -= self.alpha * self.product[i][c];
                        let r = self.residual[i][c];
                        self.sum += r * r;
                        self.maximum = self.maximum.max(r.abs());
                    }
                }
                Stage::Direction => {
                    for c in 0..3 {
                        self.direction[i][c] =
                            self.residual[i][c] + self.beta * self.direction[i][c];
                    }
                }
                _ => unreachable!(),
            }
        }
        self.position = end;
        if end != n {
            return Ok(());
        }
        match self.stage {
            Stage::Gather => self.next(Stage::Domain),
            Stage::Domain => self.next(Stage::Initialize),
            Stage::Initialize | Stage::Verify => {
                self.rho = self.sum;
                self.next(if self.maximum < 0.00001 {
                    Stage::Output
                } else {
                    Stage::Multiply
                });
            }
            Stage::Multiply => {
                if !self.sum.is_finite() || self.sum <= 0.0 || self.iterations >= 8192 {
                    return Err("Healing solver did not converge; stroke cancelled".into());
                }
                self.alpha = self.rho / self.sum;
                self.next(Stage::Update);
            }
            Stage::Update => {
                self.iterations += 1;
                self.beta = self.sum / self.rho;
                self.rho = self.sum;
                self.next(if self.maximum < 0.00001 {
                    Stage::Verify
                } else {
                    Stage::Direction
                });
            }
            Stage::Direction => self.next(Stage::Multiply),
            Stage::Output => {
                // Reuse immutable solved samples for stationary flow/endpoints,
                // but release all numerical/search scratch before retaining them.
                self.target = Vec::new();
                self.solution = Vec::new();
                self.residual = Vec::new();
                self.direction = Vec::new();
                self.product = Vec::new();
                self.candidates = Vec::new();
                self.next(Stage::Done);
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}

fn read(doc: &Document, x: i32, y: i32) -> Result<Rgba8, String> {
    if x < 0 || y < 0 || !doc.canvas_rect().contains(x as u32, y as u32) {
        return Ok(Rgba8::default());
    }
    sample(doc.raster(), x as u32, y as u32)
}
fn sample(surface: &TileSurface<Rgba8>, x: u32, y: u32) -> Result<Rgba8, String> {
    Ok(match surface.tiles().get(&(x / TILE_SIZE, y / TILE_SIZE)) {
        Some(t) => t.pin()?.pixels[((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize],
        None => surface.default,
    })
}

#[cfg(test)]
mod tests;
