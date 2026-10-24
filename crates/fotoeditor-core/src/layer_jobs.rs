//! Bounded, cancellable layer pixel jobs. The caller hydrates the next tile's
//! sources before stepping and publishes only the finished command.
use crate::*;
use std::collections::BTreeSet;
use std::sync::Arc;

pub struct ThumbnailJob {
    pub source: Document,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    coordinates: Vec<TileCoord>,
    next: usize,
}
impl ThumbnailJob {
    pub fn new(document: &Document, target: LayerId) -> Result<Self, String> {
        let mut layer = document.layer(target)?.clone();
        layer.properties.visible = true;
        layer.properties.clipped = false;
        let mut source = document.clone();
        // Adjustment thumbnails need the underlying color they transform.
        if !matches!(layer.content, LayerContent::Adjustment(_)) {
            source.layers = vec![layer];
        }
        let scale = 64.0 / f64::from(document.width.max(document.height));
        let width = (f64::from(document.width) * scale).round().max(1.0) as u32;
        let height = (f64::from(document.height) * scale).round().max(1.0) as u32;
        let mut coordinates = BTreeSet::new();
        for y in 0..height {
            for x in 0..width {
                coordinates.insert((
                    (x * document.width / width) / TILE_SIZE,
                    (y * document.height / height) / TILE_SIZE,
                ));
            }
        }
        Ok(Self {
            source,
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
            coordinates: coordinates.into_iter().collect(),
            next: 0,
        })
    }
    pub fn plan(&self) -> Result<Option<(TileCoord, CompositePlan)>, String> {
        self.coordinates
            .get(self.next)
            .copied()
            .map(|c| Ok((c, CompositePlan::tile(&self.source, c, None)?)))
            .transpose()
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if let Some((coord, plan)) = self.plan()? {
            for y in 0..self.height {
                for x in 0..self.width {
                    let sx = x * self.source.width / self.width;
                    let sy = y * self.source.height / self.height;
                    if (sx / TILE_SIZE, sy / TILE_SIZE) != coord {
                        continue;
                    }
                    let pixel = plan
                        .sample(((sy % TILE_SIZE) * TILE_SIZE + sx % TILE_SIZE) as usize)?
                        .straight();
                    self.pixels[((y * self.width + x) * 4) as usize..][..4]
                        .copy_from_slice(&pixel.0);
                }
            }
            self.next += 1;
        }
        Ok(self.next == self.coordinates.len())
    }
}

pub struct MergeRecipe {
    pub source: Document,
    after: Vec<Layer>,
    replacement: LayerId,
    label: &'static str,
}
impl MergeRecipe {
    pub fn new(document: &Document, kind: &str, target: LayerId) -> Result<Self, String> {
        let mut after = document.layers.clone();
        let mut replacement = Layer::raster(
            "Merged layer".into(),
            TileSurface::new(document.width, document.height),
        );
        let mut replacement_id = replacement.id;
        let (layers, label) = match kind {
            "mask" => {
                let original = document.layer(target)?;
                if !matches!(original.content, LayerContent::Group(_)) || original.mask.is_none() {
                    return Err("Choose a masked group".into());
                }
                if original.properties.pass_through {
                    return Err("Set the group to Normal (isolated) before rasterizing and applying its mask".into());
                }
                replacement.id = target;
                replacement_id = target;
                replacement.name = original.name.clone();
                replacement.properties = original.properties.clone();
                *find_layer_mut(&mut after, target).ok_or("Group expired")? = replacement;
                let mut source = original.clone();
                source.properties.opacity = 1.0;
                source.properties.fill = 1.0;
                source.properties.blend = BlendMode::Normal;
                source.properties.visible = true;
                source.properties.clipped = false;
                (vec![source], "Rasterize and apply group mask")
            }
            "down" => {
                let parent = parent_of(&after, target, None).ok_or("Choose a layer")?;
                let siblings = siblings_mut(&mut after, parent)?;
                let index = siblings
                    .iter()
                    .position(|l| l.id == target)
                    .ok_or("Choose a layer")?;
                if index == 0 {
                    return Err("There is no layer below this one".into());
                }
                let layers: Vec<_> = siblings.drain(index - 1..=index).collect();
                siblings.insert(index - 1, replacement);
                (layers, "Merge down")
            }
            "visible" | "flatten" => {
                fn hidden(nodes: &[Layer]) -> Vec<Layer> {
                    nodes
                        .iter()
                        .filter_map(|l| {
                            if !l.properties.visible {
                                return Some(l.clone());
                            }
                            if let LayerContent::Group(children) = &l.content {
                                let children = hidden(children);
                                if !children.is_empty() {
                                    let mut group = l.clone();
                                    group.content = LayerContent::Group(children);
                                    return Some(group);
                                }
                            }
                            None
                        })
                        .collect()
                }
                after = if kind == "visible" {
                    hidden(&document.layers)
                } else {
                    Vec::new()
                };
                after.push(replacement);
                (
                    document.layers.clone(),
                    if kind == "visible" {
                        "Merge visible"
                    } else {
                        "Flatten image"
                    },
                )
            }
            _ => return Err("Unknown merge operation".into()),
        };
        fn locks(nodes: &[Layer], document: &Document) -> Result<(), String> {
            for l in nodes {
                check_layer_edit(&document.layers, l.id, true)?;
                check_layer_edit(&document.layers, l.id, false)?;
                if let LayerContent::Group(children) = &l.content {
                    locks(children, document)?;
                }
            }
            Ok(())
        }
        locks(&layers, document)?;
        let mut source = document.clone();
        source.layers = layers;
        Ok(Self {
            source,
            after,
            replacement: replacement_id,
            label,
        })
    }
    pub fn finish(
        mut self,
        document: &Document,
        rendered: Document,
    ) -> Result<LayerCommand, String> {
        find_layer_mut(&mut self.after, self.replacement)
            .ok_or("Merge target expired")?
            .content = rendered.layers[0].content.clone();
        LayerCommand::replacing(document, self.after, self.label)
    }
}

pub fn raster_coordinates(layers: &[Layer]) -> Vec<TileCoord> {
    fn visit(nodes: &[Layer], out: &mut BTreeSet<TileCoord>) {
        for node in nodes {
            match &node.content {
                LayerContent::Raster(samples) => out.extend(samples.tiles().keys().copied()),
                LayerContent::Group(children) => visit(children, out),
                LayerContent::Adjustment(_) => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    visit(layers, &mut out);
    out.into_iter().collect()
}
impl CompositePlan {
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for step in &self.steps {
            match step {
                CompositeStep::Raster {
                    source,
                    mask,
                    adjustment,
                    ..
                } => {
                    if let Some(source) = source {
                        visitor(TileRef::Rgba(source));
                    }
                    if let Some(mask) = &mask.tile {
                        visitor(TileRef::Gray(mask));
                    }
                    if let Some((_, mask)) = adjustment {
                        if let Some(mask) = &mask.tile {
                            visitor(TileRef::Gray(mask));
                        }
                    }
                }
                CompositeStep::End { mask, .. } | CompositeStep::Adjustment { mask, .. } => {
                    if let Some(mask) = &mask.tile {
                        visitor(TileRef::Gray(mask));
                    }
                }
                _ => {}
            }
        }
    }
}

/// A composited raster candidate; no document or history mutation until finish.
pub struct LayerRenderJob {
    pub source: Document,
    pub output: Document,
    coordinates: Vec<TileCoord>,
    next: usize,
}
impl LayerRenderJob {
    pub fn updating(
        source: Document,
        output: Document,
        mut coordinates: Vec<TileCoord>,
    ) -> Result<Self, String> {
        validate_layers(&source.layers, source.width, source.height)?;
        if (source.width, source.height) != (output.width, output.height) {
            return Err("Composite cache dimensions changed".into());
        }
        coordinates.sort_unstable();
        coordinates.dedup();
        Ok(Self {
            source,
            output,
            coordinates,
            next: 0,
        })
    }
    pub fn new(source: Document) -> Result<Self, String> {
        validate_layers(&source.layers, source.width, source.height)?;
        let output = Document::blank(source.width, source.height, source.dpi)?;
        let coordinates = raster_coordinates(&source.layers);
        Ok(Self {
            source,
            output,
            coordinates,
            next: 0,
        })
    }
    pub fn plan(&self) -> Result<Option<(TileCoord, CompositePlan)>, String> {
        self.coordinates
            .get(self.next)
            .copied()
            .map(|coord| Ok((coord, CompositePlan::tile(&self.source, coord, None)?)))
            .transpose()
    }
    pub fn accept(
        &mut self,
        coord: TileCoord,
        pixels: Vec<Rgba8>,
        residency: Residency,
    ) -> Result<(), String> {
        if self.coordinates.get(self.next) != Some(&coord) || pixels.len() != TILE_PIXELS {
            return Err("Composite result does not match the pending tile".into());
        }
        residency.check_cpu((TILE_PIXELS * 4) as u64)?;
        self.output.raster_mut().insert(coord, pixels);
        self.next += 1;
        Ok(())
    }
    pub fn step(&mut self, residency: Residency) -> Result<bool, String> {
        if let Some((coord, plan)) = self.plan()? {
            let pixels = plan
                .reference(residency)?
                .into_iter()
                .map(|p| p.straight())
                .collect();
            self.accept(coord, pixels, residency)?;
        }
        Ok(self.next == self.coordinates.len())
    }
    pub fn remaining(&self) -> usize {
        self.coordinates.len() - self.next
    }
}

/// Baking a raster mask preserves the RGB bytes even where alpha becomes zero.
pub fn apply_raster_mask(
    document: &Document,
    target: LayerId,
    residency: Residency,
) -> Result<LayerCommand, String> {
    check_layer_edit(&document.layers, target, false)?;
    let mut after = document.layers.clone();
    let node = find_layer_mut(&mut after, target).ok_or("Layer no longer exists")?;
    let mask = node.mask.take().ok_or("Layer has no mask")?;
    let samples = node.raster_samples_mut()?;
    if mask.enabled {
        residency.check_cpu(samples.bytes())?;
        let coords: Vec<_> = samples.tiles().keys().copied().collect();
        for coord in coords {
            let default = mask.samples.default.0;
            let mask = mask
                .samples
                .tiles()
                .get(&coord)
                .map(|m| m.pin())
                .transpose()?;
            let mut pixels = samples.tiles()[&coord].pin()?.pixels.clone();
            for (i, pixel) in pixels.iter_mut().enumerate() {
                let value = mask.as_ref().map_or(default, |m| m.pixels[i].0);
                pixel.0[3] = ((u32::from(pixel.0[3]) * u32::from(value) + 127) / 255) as u8;
            }
            samples.insert(coord, pixels);
        }
    }
    LayerCommand::replacing(document, after, "Apply layer mask")
}

pub fn replace_raster(
    document: &Document,
    target: LayerId,
    samples: TileSurface<Rgba8>,
    label: &'static str,
) -> Result<LayerCommand, String> {
    let mut after = document.layers.clone();
    let node = find_layer_mut(&mut after, target).ok_or("Layer no longer exists")?;
    node.content = LayerContent::Raster(Arc::new(samples));
    LayerCommand::replacing(document, after, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mask_bake_is_exactly_reversible_and_preserves_hidden_rgb() {
        let mut doc = Document::blank(2, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([81, 29, 61, 200]));
        let id = doc.layers[0].id;
        let mut mask = TileSurface::with_default(2, 1, Gray8(255));
        mask.write(0, 0, Gray8(0));
        doc.layers[0].mask = Some(LayerMask {
            enabled: true,
            samples: Arc::new(mask),
        });
        let command = apply_raster_mask(&doc, id, Residency::default()).unwrap();
        command.apply(&mut doc);
        assert_eq!(doc.raster().sample(0, 0), Rgba8([81, 29, 61, 0]));
        command.revert(&mut doc);
        assert_eq!(doc.raster().sample(0, 0), Rgba8([81, 29, 61, 200]));
        assert!(doc.layers[0].mask.is_some());
    }
    #[test]
    fn composite_candidate_does_not_mutate_its_source() {
        let mut doc = Document::blank(1, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([220, 30, 10, 255]));
        let mut job = LayerRenderJob::new(doc.clone()).unwrap();
        assert!(job.step(Residency::default()).unwrap());
        assert_eq!(job.output.raster().sample(0, 0), doc.raster().sample(0, 0));
        assert_eq!(job.source.layers[0].id, doc.layers[0].id);
    }
    #[test]
    fn incremental_composite_retains_unaffected_generations_and_removes_erased_tiles() {
        let mut doc = Document::blank(512, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([10, 20, 30, 255]));
        doc.raster_mut().write(256, 0, Rgba8([40, 50, 60, 255]));
        let mut full = LayerRenderJob::new(doc.clone()).unwrap();
        while !full.step(Residency::default()).unwrap() {}
        let unchanged = full.output.raster().tiles()[&(1, 0)].id();
        doc.raster_mut().write(0, 0, Rgba8::default());
        let mut update = LayerRenderJob::updating(doc, full.output, vec![(0, 0)]).unwrap();
        assert!(update.step(Residency::default()).unwrap());
        assert!(!update.output.raster().tiles().contains_key(&(0, 0)));
        assert_eq!(update.output.raster().tiles()[&(1, 0)].id(), unchanged);
    }
    #[test]
    fn merge_commands_keep_hidden_nodes_and_undo_exact_generations() {
        let mut doc = Document::blank(2, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([90, 40, 10, 180]));
        let before = doc.raster().tiles()[&(0, 0)].id();
        let mut samples = TileSurface::new(2, 1);
        samples.write(0, 0, Rgba8([10, 190, 60, 140]));
        let top = Layer::raster("Top".into(), samples);
        let target = top.id;
        doc.layers.push(top);
        let mut hidden = Layer::raster("Hidden".into(), TileSurface::new(2, 1));
        hidden.properties.visible = false;
        let hidden_id = hidden.id;
        doc.layers
            .push(Layer::group("Hidden contents".into(), vec![hidden]));
        let expected = CompositePlan::tile(&doc, (0, 0), None)
            .unwrap()
            .reference(Residency::default())
            .unwrap()[0]
            .straight();
        for kind in ["down", "visible", "flatten"] {
            let recipe = MergeRecipe::new(&doc, kind, target).unwrap();
            let mut job = LayerRenderJob::new(recipe.source.clone()).unwrap();
            while !job.step(Residency::default()).unwrap() {}
            let command = recipe.finish(&doc, job.output).unwrap();
            let mut candidate = doc.clone();
            command.apply(&mut candidate);
            assert_eq!(
                CompositePlan::tile(&candidate, (0, 0), None)
                    .unwrap()
                    .reference(Residency::default())
                    .unwrap()[0]
                    .straight(),
                expected
            );
            if kind == "visible" {
                assert!(candidate.layer(hidden_id).is_ok());
            }
            command.revert(&mut candidate);
            assert_eq!(candidate.layers.len(), 3);
            assert_eq!(candidate.raster().tiles()[&(0, 0)].id(), before);
        }
    }
    #[test]
    fn thumbnail_samples_match_the_full_composite_and_remain_bounded() {
        let mut doc = Document::blank(512, 256, 72.0).unwrap();
        for y in 0..256 {
            for x in 0..512 {
                doc.raster_mut()
                    .write(x, y, Rgba8([x as u8, y as u8, 33, 197]));
            }
        }
        let mut job = ThumbnailJob::new(&doc, doc.layers[0].id).unwrap();
        assert_eq!((job.width, job.height), (64, 32));
        while !job.step().unwrap() {}
        assert_eq!(job.pixels.len(), 64 * 32 * 4);
        for (x, y) in [(0, 0), (31, 13), (63, 31)] {
            assert_eq!(
                &job.pixels[(y * 64 + x) * 4..][..4],
                &doc.raster().sample(x as u32 * 8, y as u32 * 8).0
            );
        }
    }
    #[test]
    fn isolated_group_mask_bake_preserves_blend_properties_and_is_reversible() {
        let mut doc = Document::blank(1, 1, 72.0).unwrap();
        doc.raster_mut().write(0, 0, Rgba8([70, 160, 210, 230]));
        let mut samples = TileSurface::new(1, 1);
        samples.write(0, 0, Rgba8([220, 50, 90, 190]));
        let mut group = Layer::group(
            "Masked group".into(),
            vec![Layer::raster("Child".into(), samples)],
        );
        group.properties.pass_through = false;
        group.properties.blend = BlendMode::Screen;
        group.properties.opacity = 0.7;
        let mut mask = TileSurface::with_default(1, 1, Gray8(255));
        mask.write(0, 0, Gray8(90));
        group.mask = Some(LayerMask {
            enabled: true,
            samples: Arc::new(mask),
        });
        let id = group.id;
        doc.layers.push(group);
        let expected = CompositePlan::tile(&doc, (0, 0), None)
            .unwrap()
            .reference(Residency::default())
            .unwrap()[0]
            .straight();
        let recipe = MergeRecipe::new(&doc, "mask", id).unwrap();
        let mut job = LayerRenderJob::new(recipe.source.clone()).unwrap();
        while !job.step(Residency::default()).unwrap() {}
        let command = recipe.finish(&doc, job.output).unwrap();
        let mut candidate = doc.clone();
        command.apply(&mut candidate);
        let actual = CompositePlan::tile(&candidate, (0, 0), None)
            .unwrap()
            .reference(Residency::default())
            .unwrap()[0]
            .straight();
        assert!(actual
            .0
            .into_iter()
            .zip(expected.0)
            .all(|(a, b)| a.abs_diff(b) <= 1));
        assert!(candidate.layer(id).unwrap().mask.is_none());
        assert_eq!(
            candidate.layer(id).unwrap().properties.blend,
            BlendMode::Screen
        );
        command.revert(&mut candidate);
        assert!(matches!(
            candidate.layer(id).unwrap().content,
            LayerContent::Group(_)
        ));
    }
}
