use fotoeditor_core::{Gray8, TileRef, TileSurface, TILE_SIZE};
use serde::Deserialize;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaskOperation {
    Invert,
    Feather,
    Grow,
    Shrink,
    Border,
    Smooth,
}
#[derive(Clone, Copy)]
enum Kernel {
    Min,
    Max,
    Mean,
    Invert,
    Difference,
}
#[derive(Clone, Copy)]
struct Pass {
    vertical: bool,
    kernel: Kernel,
    radius: u32,
}
/// Separable rolling-window passes, one bounded document row/column per step.
/// No dense float mask or image-wide JavaScript buffer is used.
pub struct MaskJob {
    pub original: Arc<TileSurface<Gray8>>,
    pub input: Arc<TileSurface<Gray8>>,
    pub output: TileSurface<Gray8>,
    passes: VecDeque<Pass>,
    grown: Option<TileSurface<Gray8>>,
    border: bool,
    next: u32,
    available: u64,
    done: bool,
    compacting: bool,
}
impl MaskJob {
    pub fn finish(self) -> Result<TileSurface<Gray8>, String> {
        if !self.done() {
            return Err("Selection operation is incomplete".into());
        }
        Ok(self.output)
    }
    pub fn new(
        input: Arc<TileSurface<Gray8>>,
        operation: MaskOperation,
        radius: u32,
        available: u64,
    ) -> Result<Self, String> {
        fotoeditor_core::validate_size(input.width, input.height, 72.0)?;
        if radius > 256 {
            return Err("Selection radius must be 0–256 pixels".into());
        }
        let mut passes = VecDeque::new();
        let mut pair = |kernel, radius| {
            passes.push_back(Pass {
                vertical: false,
                kernel,
                radius,
            });
            passes.push_back(Pass {
                vertical: true,
                kernel,
                radius,
            });
        };
        match operation {
            MaskOperation::Invert => passes.push_back(Pass {
                vertical: false,
                kernel: Kernel::Invert,
                radius: 0,
            }),
            MaskOperation::Grow | MaskOperation::Border => pair(Kernel::Max, radius),
            MaskOperation::Shrink => pair(Kernel::Min, radius),
            MaskOperation::Smooth => {
                pair(Kernel::Min, radius);
                pair(Kernel::Max, radius);
                pair(Kernel::Max, radius);
                pair(Kernel::Min, radius);
            }
            MaskOperation::Feather => {
                // Three odd-width box passes approximate a Gaussian with sigma=radius.
                let sigma = f64::from(radius);
                let mut lower = (4.0 * sigma * sigma + 1.0).sqrt().floor() as u32;
                if lower.is_multiple_of(2) {
                    lower = lower.saturating_sub(1);
                }
                lower = lower.max(1);
                let l = f64::from(lower);
                let count = ((12.0 * sigma * sigma - 3.0 * l * l - 12.0 * l - 9.0)
                    / (-4.0 * l - 4.0))
                    .round()
                    .clamp(0.0, 3.0) as u32;
                for i in 0..3 {
                    pair(
                        Kernel::Mean,
                        if i < count {
                            (lower - 1) / 2
                        } else {
                            lower.div_ceil(2)
                        },
                    );
                }
            }
        }
        let default = if matches!(operation, MaskOperation::Invert) {
            Gray8(255 - input.default.0)
        } else {
            input.default
        };
        let output = TileSurface::with_default(input.width, input.height, default);
        let mut job = Self {
            original: input.clone(),
            input,
            output,
            passes,
            grown: None,
            border: matches!(operation, MaskOperation::Border),
            next: 0,
            available,
            done: false,
            compacting: false,
        };
        if radius == 0 && !matches!(operation, MaskOperation::Invert | MaskOperation::Border) {
            job.output = (*job.input).clone();
            job.done = true;
            job.passes.clear();
            return Ok(job);
        }
        if job.temporary_bytes() + 2 * 65536 > available {
            return Err("Not enough memory for selection operation".into());
        }
        Ok(job)
    }
    fn coordinates(&self) -> Vec<(u32, u32)> {
        if self.done || self.compacting {
            return Vec::new();
        }
        let pass = self.passes[0];
        if pass.vertical {
            (0..self.input.height.div_ceil(TILE_SIZE))
                .map(|y| (self.next / TILE_SIZE, y))
                .collect()
        } else {
            (0..self.input.width.div_ceil(TILE_SIZE))
                .map(|x| (x, self.next / TILE_SIZE))
                .collect()
        }
    }
    pub fn needed(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for c in self.coordinates() {
            if let Some(t) = self.input.tiles().get(&c) {
                visitor(TileRef::Gray(t));
            }
            if let Some(t) = self.grown.as_ref().and_then(|s| s.tiles().get(&c)) {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn visit_candidates(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for t in self
            .input
            .tiles()
            .values()
            .chain(self.output.tiles().values())
        {
            visitor(TileRef::Gray(t));
        }
        if let Some(s) = &self.grown {
            for t in s.tiles().values() {
                visitor(TileRef::Gray(t));
            }
        }
    }
    pub fn temporary_bytes(&self) -> u64 {
        let mut seen: HashSet<_> = self.original.tiles().values().map(|t| t.id()).collect();
        let mut count = 0;
        self.visit_candidates(&mut |t| {
            if seen.insert(t.id()) {
                count += t.bytes();
            }
        });
        count + u64::from(self.input.width.max(self.input.height) + 1024) * 32 + 4 * 65536
    }
    pub fn done(&self) -> bool {
        self.done
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done {
            return Ok(true);
        }
        if self.compacting {
            let columns = self.output.width.div_ceil(TILE_SIZE);
            self.output
                .compact_tile((self.next % columns, self.next / columns))?;
            self.next += 1;
            if self.next == columns * self.output.height.div_ceil(TILE_SIZE) {
                self.end_pass();
            }
            return Ok(self.done);
        }
        let pass = self.passes[0];
        let length = if pass.vertical {
            self.input.height
        } else {
            self.input.width
        };
        // At most one new tile per row segment/column segment, plus line scratch.
        if self.temporary_bytes() + u64::from(length.div_ceil(TILE_SIZE)) * 65536 > self.available {
            return Err("Selection operation exceeds the shared CPU budget".into());
        }
        let coords = self.coordinates();
        let tiles = coords
            .iter()
            .map(|c| self.input.tiles().get(c).map(|t| t.pin()).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        let grown = coords
            .iter()
            .map(|c| {
                self.grown
                    .as_ref()
                    .and_then(|s| s.tiles().get(c))
                    .map(|t| t.pin())
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut line = Vec::with_capacity(length as usize);
        for i in 0..length {
            let index = if pass.vertical {
                ((i % TILE_SIZE) * TILE_SIZE + self.next % TILE_SIZE) as usize
            } else {
                ((self.next % TILE_SIZE) * TILE_SIZE + i % TILE_SIZE) as usize
            };
            line.push(
                tiles[(i / TILE_SIZE) as usize]
                    .as_ref()
                    .map_or(self.input.default.0, |t| t.pixels[index].0),
            );
        }
        let mut filtered = filter_line(&line, pass.kernel, pass.radius);
        if matches!(pass.kernel, Kernel::Difference) {
            let default = self.grown.as_ref().map_or(0, |s| s.default.0);
            for i in 0..length {
                let index = if pass.vertical {
                    ((i % TILE_SIZE) * TILE_SIZE + self.next % TILE_SIZE) as usize
                } else {
                    ((self.next % TILE_SIZE) * TILE_SIZE + i % TILE_SIZE) as usize
                };
                filtered[i as usize] = grown[(i / TILE_SIZE) as usize]
                    .as_ref()
                    .map_or(default, |t| t.pixels[index].0)
                    .saturating_sub(line[i as usize]);
            }
        }
        for (tile_index, c) in coords.iter().enumerate() {
            let start = tile_index as u32 * TILE_SIZE;
            let end = length.min((tile_index as u32 + 1) * TILE_SIZE);
            if !self.output.tiles().contains_key(c)
                && filtered[start as usize..end as usize]
                    .iter()
                    .all(|v| *v == self.output.default.0)
            {
                continue;
            }
            let mut target = self.output.edit_tile(*c);
            for i in tile_index as u32 * TILE_SIZE..length.min((tile_index as u32 + 1) * TILE_SIZE)
            {
                let index = if pass.vertical {
                    ((i % TILE_SIZE) * TILE_SIZE + self.next % TILE_SIZE) as usize
                } else {
                    ((self.next % TILE_SIZE) * TILE_SIZE + i % TILE_SIZE) as usize
                };
                target[index] = Gray8(filtered[i as usize]);
            }
        }
        self.next += 1;
        let lines = if pass.vertical {
            self.input.width
        } else {
            self.input.height
        };
        if self.next == lines {
            self.compacting = true;
            self.next = 0;
        }
        Ok(self.done)
    }
    fn end_pass(&mut self) {
        let pass = self.passes[0];
        self.compacting = false;
        self.output.drain_dirty();
        self.passes.pop_front();
        self.next = 0;
        if self.border && self.passes.is_empty() {
            self.border = false;
            self.grown = Some(std::mem::replace(
                &mut self.output,
                TileSurface::with_default(
                    self.input.width,
                    self.input.height,
                    self.original.default,
                ),
            ));
            self.input = self.original.clone();
            self.passes.extend([
                Pass {
                    vertical: false,
                    kernel: Kernel::Min,
                    radius: pass.radius,
                },
                Pass {
                    vertical: true,
                    kernel: Kernel::Min,
                    radius: pass.radius,
                },
                Pass {
                    vertical: false,
                    kernel: Kernel::Difference,
                    radius: 0,
                },
            ]);
        } else if self.passes.is_empty() {
            self.done = true;
        } else {
            let default = if matches!(self.passes[0].kernel, Kernel::Difference) {
                Gray8(
                    self.grown
                        .as_ref()
                        .map_or(0, |s| s.default.0)
                        .saturating_sub(self.output.default.0),
                )
            } else {
                self.output.default
            };
            self.input = Arc::new(std::mem::replace(
                &mut self.output,
                TileSurface::with_default(self.input.width, self.input.height, default),
            ));
        }
    }
}
fn filter_line(input: &[u8], kernel: Kernel, radius: u32) -> Vec<u8> {
    if matches!(kernel, Kernel::Invert) {
        return input.iter().map(|v| 255 - v).collect();
    }
    if matches!(kernel, Kernel::Difference) || radius == 0 {
        return input.to_vec();
    }
    let r = radius as usize;
    let width = 2 * r + 1;
    let mut padded = vec![input[0]; r];
    padded.extend_from_slice(input);
    padded.resize(input.len() + 2 * r, input[input.len() - 1]);
    let mut output = Vec::with_capacity(input.len());
    if matches!(kernel, Kernel::Mean) {
        let mut sum: u32 = padded[..width].iter().map(|v| u32::from(*v)).sum();
        for i in 0..input.len() {
            output.push(((sum + width as u32 / 2) / width as u32) as u8);
            if i + width < padded.len() {
                sum = sum - u32::from(padded[i]) + u32::from(padded[i + width]);
            }
        }
    } else {
        let mut q: VecDeque<usize> = VecDeque::with_capacity(width);
        for i in 0..padded.len() {
            while q.front().is_some_and(|j| *j + width <= i) {
                q.pop_front();
            }
            while q.back().is_some_and(|j| {
                if matches!(kernel, Kernel::Min) {
                    padded[*j] >= padded[i]
                } else {
                    padded[*j] <= padded[i]
                }
            }) {
                q.pop_back();
            }
            q.push_back(i);
            if i + 1 >= width {
                output.push(padded[q[0]]);
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_full_masks_and_inverted_holes_preserve_their_default_plane() {
        let full = Arc::new(TileSurface::with_default(513, 9, Gray8(255)));
        for op in [
            MaskOperation::Feather,
            MaskOperation::Grow,
            MaskOperation::Shrink,
            MaskOperation::Smooth,
        ] {
            let s = run(full.clone(), op, 2);
            assert_eq!(s.default, Gray8(255));
            assert_eq!(s.bytes(), 0);
        }
        let empty = run(full.clone(), MaskOperation::Invert, 0);
        assert_eq!(empty.default, Gray8(0));
        assert_eq!(empty.bytes(), 0);
        let mut hole = (*full).clone();
        hole.write(256, 4, Gray8(0));
        let border = run(Arc::new(hole), MaskOperation::Border, 1);
        assert_eq!(border.sample(256, 4), Gray8(255));
        assert_eq!(border.sample(255, 3), Gray8(255));
        assert_eq!(border.sample(254, 3), Gray8(0));
    }
    fn run(input: Arc<TileSurface<Gray8>>, op: MaskOperation, r: u32) -> TileSurface<Gray8> {
        let mut j = MaskJob::new(input, op, r, 32 * 1024 * 1024).unwrap();
        while !j.step().unwrap() {}
        j.output
    }
    #[test]
    fn grow_shrink_border_and_invert_have_exact_independent_expected_samples() {
        let mut s = TileSurface::new(513, 9);
        s.write(256, 4, Gray8(255));
        let s = Arc::new(s);
        let grown = Arc::new(run(s.clone(), MaskOperation::Grow, 2));
        for y in 0..9 {
            for x in 0..513 {
                assert_eq!(
                    grown.sample(x, y).0,
                    if (254..=258).contains(&x) && (2..=6).contains(&y) {
                        255
                    } else {
                        0
                    }
                );
            }
        }
        let shrunk = run(grown.clone(), MaskOperation::Shrink, 2);
        assert_eq!(shrunk.sample(256, 4).0, 255);
        assert_eq!(shrunk.sample(255, 4).0, 0);
        let border = run(grown, MaskOperation::Border, 1);
        assert_eq!(border.sample(256, 4).0, 0);
        assert_eq!(border.sample(253, 4).0, 255);
        let inverse = run(s, MaskOperation::Invert, 0);
        assert_eq!(inverse.sample(256, 4).0, 0);
        assert_eq!(inverse.sample(0, 0).0, 255);
    }
    #[test]
    fn feather_is_symmetric_monotone_and_crosses_tile_boundaries() {
        let mut s = TileSurface::new(513, 17);
        for y in 0..17 {
            for x in 240..=272 {
                s.write(x, y, Gray8(255));
            }
        }
        let f = run(Arc::new(s), MaskOperation::Feather, 3);
        for d in 0..32 {
            assert_eq!(f.sample(256 - d, 8), f.sample(256 + d, 8));
        }
        assert!(f.sample(239, 8).0 > 0);
        assert!(f.sample(240, 8).0 > f.sample(239, 8).0);
        assert!(f.sample(256, 8).0 > 250);
    }
}
