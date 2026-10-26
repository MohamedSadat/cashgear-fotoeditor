//! Point-only layer auto-selection using the compositor's ordered alpha plan.
use crate::*;
use std::sync::Arc;
#[derive(Clone, Copy, Default)]
struct Hit {
    alpha: f64,
    layer: Option<LayerId>,
}
pub struct HitTestJob {
    source: Arc<Document>,
    plan: CompositePlan,
    index: usize,
    pixel: usize,
    current: Hit,
    stack: Vec<(Hit, bool)>,
}
impl HitTestJob {
    pub fn new(source: Arc<Document>, point: [f64; 2]) -> Result<Self, String> {
        if point.iter().any(|v| !v.is_finite()) {
            return Err("Layer hit-test point must be finite".into());
        }
        let inside = source.canvas_rect().geometry();
        let valid = point[0] >= inside.x
            && point[1] >= inside.y
            && point[0] < inside.x + inside.width
            && point[1] < inside.y + inside.height;
        let x = point[0].max(0.0).floor() as u32;
        let y = point[1].max(0.0).floor() as u32;
        let plan = if valid {
            CompositePlan::tile(&source, (x / TILE_SIZE, y / TILE_SIZE), None)?
        } else {
            CompositePlan {
                steps: vec![],
                stack_depth: 0,
            }
        };
        Ok(Self {
            source,
            plan,
            index: 0,
            pixel: ((y % TILE_SIZE) * TILE_SIZE + x % TILE_SIZE) as usize,
            current: Hit::default(),
            stack: vec![],
        })
    }
    pub fn done(&self) -> bool {
        self.index == self.plan.steps.len()
    }
    pub fn scratch_bytes(&self) -> u64 {
        1024 * 1024 + self.plan.steps.len() as u64 * 4096
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.source.visit_tiles(v);
    }
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        match self.plan.steps.get(self.index) {
            Some(CompositeStep::Raster { source, mask, .. }) => {
                if let Some(t) = source {
                    v(TileRef::Rgba(t));
                }
                if let Some(t) = &mask.tile {
                    v(TileRef::Gray(t));
                }
            }
            Some(CompositeStep::End { mask, .. }) => {
                if let Some(t) = &mask.tile {
                    v(TileRef::Gray(t));
                }
            }
            _ => {}
        }
    }
    pub fn step(&mut self) -> Result<bool, String> {
        let Some(step) = self.plan.steps.get(self.index) else {
            return Ok(true);
        };
        match step {
            CompositeStep::Begin { pass_through } => {
                self.stack.push((self.current, *pass_through));
                if !pass_through {
                    self.current = Hit::default();
                }
            }
            CompositeStep::Raster {
                id,
                source,
                mask,
                properties,
                clipped,
                ..
            } => {
                if let Some(t) = source {
                    let alpha = t.pin()?.pixels[self.pixel].0[3] as f64 / 255.0
                        * mask.sample(self.pixel)?
                        * properties.opacity
                        * properties.fill;
                    if alpha > 0.0 && (!clipped || self.current.alpha > 0.0) {
                        if !clipped {
                            self.current.alpha = alpha + self.current.alpha * (1.0 - alpha);
                        }
                        if check_layer_edit(&self.source.layers, *id, true).is_ok() {
                            self.current.layer = Some(*id);
                        }
                    }
                }
            }
            CompositeStep::End {
                mask,
                properties,
                clipped,
            } => {
                let factor = mask.sample(self.pixel)? * properties.opacity * properties.fill;
                let (backdrop, pass) =
                    *self.stack.last().ok_or("Unbalanced layer hit-test plan")?;
                let source = self.current;
                self.current = if pass {
                    Hit {
                        alpha: backdrop.alpha + (source.alpha - backdrop.alpha) * factor,
                        layer: if factor > 0.0 {
                            source.layer
                        } else {
                            backdrop.layer
                        },
                    }
                } else {
                    let alpha = source.alpha * factor;
                    Hit {
                        alpha: if *clipped {
                            backdrop.alpha
                        } else {
                            alpha + backdrop.alpha * (1.0 - alpha)
                        },
                        layer: if alpha > 0.0 && (!clipped || backdrop.alpha > 0.0) {
                            source.layer.or(backdrop.layer)
                        } else {
                            backdrop.layer
                        },
                    }
                };
                self.stack.pop();
            }
            CompositeStep::Adjustment { .. } => {}
        }
        self.index += 1;
        Ok(self.done())
    }
    pub fn finish(self) -> Result<Option<LayerId>, String> {
        if !self.done() {
            return Err("Layer hit test is incomplete".into());
        }
        Ok(self.current.layer)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hit(d: &Document, p: [f64; 2]) -> Option<LayerId> {
        let mut j = HitTestJob::new(Arc::new(d.clone()), p).unwrap();
        while !j.step().unwrap() {}
        j.finish().unwrap()
    }
    #[test]
    fn rendered_alpha_masks_groups_clipping_and_position_locks_control_hits() {
        let mut d = Document::blank(4, 3, 72.0).unwrap();
        d.raster_mut().write(1, 1, Rgba8([30, 40, 50, 255]));
        let bottom = d.layers[0].id;
        let mut samples = TileSurface::new(4, 3);
        samples.write(1, 1, Rgba8([255, 0, 0, 1]));
        samples.write(2, 1, Rgba8([255, 0, 0, 255]));
        let child = Layer::raster("top".into(), samples);
        let top = child.id;
        let mut group = Layer::group("group".into(), vec![child]);
        let mut mask = TileSurface::with_default(4, 3, Gray8(255));
        mask.write(1, 1, Gray8(0));
        group.mask = Some(LayerMask {
            enabled: true,
            samples: Arc::new(mask),
        });
        d.layers.push(group);
        assert_eq!(hit(&d, [1.5, 1.5]), Some(bottom));
        assert_eq!(hit(&d, [2.5, 1.5]), Some(top));
        d.layers[1].mask.as_mut().unwrap().enabled = false;
        assert_eq!(hit(&d, [1.5, 1.5]), Some(top));
        d.layers[1].properties.opacity = 0.0;
        assert_eq!(hit(&d, [1.5, 1.5]), Some(bottom));
        d.layers[1].properties.opacity = 1.0;
        d.layers[1].properties.clipped = true;
        assert_eq!(hit(&d, [2.5, 1.5]), None);
        assert_eq!(hit(&d, [1.5, 1.5]), Some(top));
        d.layers[1].properties.locks.position = true;
        assert_eq!(hit(&d, [1.5, 1.5]), Some(bottom));
        assert_eq!(hit(&d, [-1.0, 1.0]), None);
    }
}
