//! Tile-sized CPU reference and an ordered plan shared with the GPU adapter.
//! Planning never hydrates pixels; the worker can load one source at a time.
use crate::{
    AdjustmentKernel, BlendMode, Composite, Document, Gray8, Layer, LayerContent, LayerProperties,
    Residency, Rgba8, TileCoord, TileHandle, TILE_PIXELS, TILE_SIZE,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct CompositeMask {
    pub tile: Option<TileHandle<Gray8>>,
    pub default: u8,
}
impl Default for CompositeMask {
    fn default() -> Self {
        Self {
            tile: None,
            default: 255,
        }
    }
}
impl CompositeMask {
    pub fn sample(&self, index: usize) -> Result<f64, String> {
        Ok(f64::from(
            self.tile
                .as_ref()
                .map(|t| t.pin().map(|t| t.pixels[index].0))
                .transpose()?
                .unwrap_or(self.default),
        ) / 255.0)
    }
}

#[derive(Clone)]
pub enum CompositeStep {
    Begin {
        pass_through: bool,
    },
    Raster {
        id: crate::LayerId,
        source: Option<TileHandle<Rgba8>>,
        mask: CompositeMask,
        properties: LayerProperties,
        clipped: bool,
        adjustment: Option<(Arc<AdjustmentKernel>, CompositeMask)>,
    },
    Adjustment {
        kernel: Arc<AdjustmentKernel>,
        mask: CompositeMask,
        properties: LayerProperties,
    },
    End {
        mask: CompositeMask,
        properties: LayerProperties,
        clipped: bool,
    },
}
pub struct CompositePlan {
    pub steps: Vec<CompositeStep>,
    pub stack_depth: usize,
}
fn mask(layer: &Layer, coord: TileCoord) -> CompositeMask {
    layer
        .mask
        .as_ref()
        .filter(|m| m.enabled)
        .map_or_else(CompositeMask::default, |m| CompositeMask {
            tile: m.samples.tiles().get(&coord).cloned(),
            default: m.samples.default.0,
        })
}
fn adjust(
    backdrop: Composite,
    kernel: &AdjustmentKernel,
    mode: BlendMode,
    factor: f64,
) -> Composite {
    if backdrop.0[3] <= 0.0 {
        return backdrop;
    }
    let mut before = backdrop.straight();
    // A positive inherited alpha may round below one byte; color transforms
    // still apply while the original floating-point alpha stays unchanged.
    before.0[3] = 255;
    let after = kernel.sample(before, 255);
    let b = std::array::from_fn(|i| f64::from(before.0[i]) / 255.0);
    let s = std::array::from_fn(|i| f64::from(after.0[i]) / 255.0);
    let rgb = crate::blend_rgb(mode, b, s);
    let mut out = backdrop;
    for (i, value) in rgb.into_iter().enumerate() {
        out.0[i] += (value * backdrop.0[3] - out.0[i]) * factor.clamp(0.0, 1.0);
    }
    out
}
impl CompositePlan {
    /// Low-resolution worker previews evaluate only requested samples, using the
    /// same tree semantics without allocating a full-resolution composite.
    pub fn sample(&self, index: usize) -> Result<Composite, String> {
        if index >= TILE_PIXELS {
            return Err("Composite sample outside tile".into());
        }
        let mut current = Composite::default();
        let mut stack = Vec::new();
        for step in &self.steps {
            match step {
                CompositeStep::Begin { pass_through } => {
                    stack.push((current, *pass_through));
                    if !pass_through {
                        current = Composite::default();
                    }
                }
                CompositeStep::Raster {
                    source,
                    mask,
                    properties,
                    clipped,
                    adjustment,
                    ..
                } => {
                    if let Some(source) = source {
                        let factor = properties.opacity * properties.fill * mask.sample(index)?;
                        let mut source = source.pin()?.pixels[index];
                        if let Some((kernel, coverage)) = adjustment {
                            source = kernel
                                .sample(source, (coverage.sample(index)? * 255.0).round() as u8);
                        }
                        current = current.blend(
                            Composite::from_straight(source),
                            properties.blend,
                            factor,
                            *clipped,
                        );
                    }
                }
                CompositeStep::Adjustment {
                    kernel,
                    mask,
                    properties,
                } => {
                    current = adjust(
                        current,
                        kernel,
                        properties.blend,
                        properties.opacity * properties.fill * mask.sample(index)?,
                    );
                }
                CompositeStep::End {
                    mask,
                    properties,
                    clipped,
                } => {
                    let (backdrop, pass_through) =
                        stack.pop().ok_or("Unbalanced composite plan")?;
                    let factor = properties.opacity * properties.fill * mask.sample(index)?;
                    current = if pass_through {
                        backdrop.interpolate(current, factor)
                    } else {
                        backdrop.blend(current, properties.blend, factor, *clipped)
                    };
                }
            }
        }
        Ok(current)
    }
    pub fn tile(
        document: &Document,
        coord: TileCoord,
        preview: Option<&Layer>,
    ) -> Result<Self, String> {
        if coord.0 >= document.width.div_ceil(TILE_SIZE)
            || coord.1 >= document.height.div_ceil(TILE_SIZE)
        {
            return Err("Composite tile lies outside the document".into());
        }
        fn node(
            layer: &Layer,
            coord: TileCoord,
            preview: Option<&Layer>,
            steps: &mut Vec<CompositeStep>,
            clipped: bool,
            normal: bool,
        ) -> Result<(), String> {
            let layer = preview.filter(|p| p.id == layer.id).unwrap_or(layer);
            let mut properties = layer.properties.clone();
            if normal {
                properties.blend = BlendMode::Normal;
            }
            match &layer.content {
                LayerContent::Raster(samples) => steps.push(CompositeStep::Raster {
                    id: layer.id,
                    source: samples.tiles().get(&coord).cloned(),
                    mask: mask(layer, coord),
                    properties,
                    clipped,
                    adjustment: None,
                }),
                LayerContent::Adjustment(adjustment) => steps.push(CompositeStep::Adjustment {
                    kernel: Arc::new(adjustment.compile()?),
                    mask: mask(layer, coord),
                    properties,
                }),
                LayerContent::Group(children) => {
                    // A clipped group is an isolated source for source-atop.
                    steps.push(CompositeStep::Begin {
                        pass_through: properties.pass_through && !clipped,
                    });
                    siblings(children, coord, preview, steps)?;
                    steps.push(CompositeStep::End {
                        mask: mask(layer, coord),
                        properties,
                        clipped,
                    });
                }
            }
            Ok(())
        }
        fn siblings(
            layers: &[Layer],
            coord: TileCoord,
            preview: Option<&Layer>,
            steps: &mut Vec<CompositeStep>,
        ) -> Result<(), String> {
            let mut i = 0;
            while i < layers.len() {
                let base = &layers[i];
                if base.properties.clipped {
                    i += 1;
                    continue;
                } // No base in this parent.
                let mut end = i + 1;
                while end < layers.len() && layers[end].properties.clipped {
                    end += 1;
                }
                if base.properties.visible {
                    if end > i + 1 {
                        steps.push(CompositeStep::Begin {
                            pass_through: false,
                        });
                        node(base, coord, preview, steps, false, true)?;
                        for clipped in &layers[i + 1..end] {
                            if clipped.properties.visible {
                                node(clipped, coord, preview, steps, true, false)?;
                            }
                        }
                        steps.push(CompositeStep::End {
                            mask: CompositeMask::default(),
                            properties: LayerProperties {
                                blend: base.properties.blend,
                                ..LayerProperties::default()
                            },
                            clipped: false,
                        });
                    } else {
                        node(base, coord, preview, steps, false, false)?;
                    }
                }
                i = end;
            }
            Ok(())
        }
        let mut steps = Vec::new();
        siblings(&document.layers, coord, preview, &mut steps)?;
        let mut depth = 0;
        let mut stack_depth = 0;
        for step in &steps {
            match step {
                CompositeStep::Begin { .. } => {
                    depth += 1;
                    stack_depth = stack_depth.max(depth);
                }
                CompositeStep::End { .. } => {
                    depth -= 1;
                }
                _ => {}
            }
        }
        Ok(Self { steps, stack_depth })
    }
    pub fn scratch_bytes(&self) -> u64 {
        ((self.stack_depth + 1) * TILE_PIXELS * size_of::<Composite>()) as u64
    }
    pub fn reference(&self, residency: Residency) -> Result<Vec<Composite>, String> {
        residency.check_cpu(self.scratch_bytes())?;
        let mut current = vec![Composite::default(); TILE_PIXELS];
        let mut stack = Vec::new();
        for step in &self.steps {
            match step {
                CompositeStep::Begin { pass_through } => {
                    let next = if *pass_through {
                        current.clone()
                    } else {
                        vec![Composite::default(); TILE_PIXELS]
                    };
                    stack.push((current, *pass_through));
                    current = next;
                }
                CompositeStep::Raster {
                    source,
                    mask,
                    properties,
                    clipped,
                    adjustment,
                    ..
                } => {
                    let Some(source) = source else {
                        continue;
                    };
                    let source = source.pin()?;
                    let mask_default = f64::from(mask.default) / 255.0;
                    let mask = mask.tile.as_ref().map(|m| m.pin()).transpose()?;
                    let coverage = adjustment
                        .as_ref()
                        .map(|(_, m)| m.tile.as_ref().map(|m| m.pin()).transpose())
                        .transpose()?
                        .flatten();
                    let factor = properties.opacity * properties.fill;
                    for (i, pixel) in current.iter_mut().enumerate() {
                        let m = mask
                            .as_ref()
                            .map_or(mask_default, |m| f64::from(m.pixels[i].0) / 255.0);
                        let mut source = source.pixels[i];
                        if let Some((kernel, mask)) = adjustment {
                            source = kernel.sample(
                                source,
                                coverage.as_ref().map_or(mask.default, |m| m.pixels[i].0),
                            );
                        }
                        *pixel = pixel.blend(
                            Composite::from_straight(source),
                            properties.blend,
                            factor * m,
                            *clipped,
                        );
                    }
                }
                CompositeStep::Adjustment {
                    kernel,
                    mask,
                    properties,
                } => {
                    let default = f64::from(mask.default) / 255.0;
                    let mask = mask.tile.as_ref().map(|m| m.pin()).transpose()?;
                    for (i, pixel) in current.iter_mut().enumerate() {
                        *pixel = adjust(
                            *pixel,
                            kernel,
                            properties.blend,
                            properties.opacity
                                * properties.fill
                                * mask
                                    .as_ref()
                                    .map_or(default, |m| f64::from(m.pixels[i].0) / 255.0),
                        );
                    }
                }
                CompositeStep::End {
                    mask,
                    properties,
                    clipped,
                } => {
                    let (mut backdrop, pass_through) =
                        stack.pop().expect("balanced composite plan");
                    let mask_default = f64::from(mask.default) / 255.0;
                    let mask = mask.tile.as_ref().map(|m| m.pin()).transpose()?;
                    let factor = properties.opacity * properties.fill;
                    for (i, pixel) in backdrop.iter_mut().enumerate() {
                        let m = mask
                            .as_ref()
                            .map_or(mask_default, |m| f64::from(m.pixels[i].0) / 255.0);
                        *pixel = if pass_through {
                            pixel.interpolate(current[i], factor * m)
                        } else {
                            pixel.blend(current[i], properties.blend, factor * m, *clipped)
                        };
                    }
                    current = backdrop;
                }
            }
        }
        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LayerMask, TileSurface};
    use std::sync::Arc;
    fn raster(name: &str, pixel: Rgba8, blend: BlendMode) -> Layer {
        let mut samples = TileSurface::new(1, 1);
        samples.write(0, 0, pixel);
        let mut layer = Layer::raster(name.into(), samples);
        layer.properties.blend = blend;
        layer
    }
    fn result(layers: Vec<Layer>) -> Rgba8 {
        let document = Document {
            selection: None,
            width: 1,
            height: 1,
            dpi: 72.0,
            layers,
        };
        CompositePlan::tile(&document, (0, 0), None)
            .unwrap()
            .reference(Residency::default())
            .unwrap()[0]
            .straight()
    }
    #[test]
    fn isolated_and_pass_through_groups_have_independent_goldens() {
        let red = raster("Red", Rgba8([255, 0, 0, 255]), BlendMode::Normal);
        let blue = raster("Blue", Rgba8([0, 0, 255, 255]), BlendMode::Multiply);
        let mut group = Layer::group("Group".into(), vec![blue]);
        assert_eq!(
            result(vec![red.clone(), group.clone()]),
            Rgba8([0, 0, 0, 255])
        );
        group.properties.opacity = 0.5;
        assert_eq!(
            result(vec![red.clone(), group.clone()]),
            Rgba8([128, 0, 0, 255])
        );
        group.properties.pass_through = false;
        assert_eq!(result(vec![red, group]), Rgba8([128, 0, 128, 255]));
    }
    #[test]
    fn clipping_uses_the_immediate_base_shape_and_cannot_increase_its_alpha() {
        let base = raster("Base", Rgba8([255, 0, 0, 128]), BlendMode::Normal);
        let mut clip = raster("Clip", Rgba8([0, 0, 255, 255]), BlendMode::Normal);
        clip.properties.clipped = true;
        assert_eq!(
            result(vec![base.clone(), clip.clone()]),
            Rgba8([0, 0, 255, 128])
        );
        assert_eq!(result(vec![clip.clone()]), Rgba8::default());
        let mut hidden = base;
        hidden.properties.visible = false;
        assert_eq!(result(vec![hidden, clip]), Rgba8::default());
    }
    #[test]
    fn masks_and_group_masks_scale_coverage_and_disabled_masks_do_not() {
        let mut samples = TileSurface::with_default(1, 1, Gray8(255));
        samples.write(0, 0, Gray8(128));
        let mask = LayerMask {
            enabled: true,
            samples: Arc::new(samples),
        };
        let mut layer = raster("Masked", Rgba8([87, 33, 19, 255]), BlendMode::Normal);
        layer.mask = Some(mask.clone());
        assert_eq!(result(vec![layer.clone()]), Rgba8([87, 33, 19, 128]));
        let mut group = Layer::group("Masked group".into(), vec![layer.clone()]);
        group.mask = Some(mask);
        assert_eq!(result(vec![group]), Rgba8([87, 33, 19, 64]));
        layer.mask.as_mut().unwrap().enabled = false;
        assert_eq!(result(vec![layer]), Rgba8([87, 33, 19, 255]));
    }
}
