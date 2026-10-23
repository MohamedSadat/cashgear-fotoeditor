//! Layer trees and metadata commands. Pixel jobs hydrate only their working tiles.
use crate::{BlendMode, Command, Document, Gray8, Rgba8, TileHandle, TileSurface};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub type LayerId = u32;
pub const MAX_LAYERS: usize = 512;
pub const MAX_GROUP_DEPTH: usize = 32;
static NEXT_LAYER: AtomicU32 = AtomicU32::new(1);
pub fn reserve_layer_ids(maximum: LayerId) -> Result<(), String> {
    if maximum >= i32::MAX as u32 {
        return Err("Layer ID exceeds the supported range".into());
    }
    NEXT_LAYER.fetch_max(maximum + 1, Ordering::Relaxed);
    Ok(())
}
fn next_id() -> LayerId {
    NEXT_LAYER.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerLocks {
    pub pixels: bool,
    pub alpha: bool,
    pub position: bool,
    pub all: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LayerProperties {
    pub visible: bool,
    pub opacity: f64,
    pub fill: f64,
    pub blend: BlendMode,
    pub pass_through: bool,
    pub clipped: bool,
    pub locks: LayerLocks,
}
impl Default for LayerProperties {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 1.0,
            fill: 1.0,
            blend: BlendMode::Normal,
            pass_through: false,
            clipped: false,
            locks: LayerLocks::default(),
        }
    }
}
#[derive(Clone)]
pub struct LayerMask {
    pub enabled: bool,
    pub samples: Arc<TileSurface<Gray8>>,
}
#[derive(Clone)]
pub enum LayerContent {
    Raster(Arc<TileSurface<Rgba8>>),
    Group(Vec<Layer>),
}
#[derive(Clone)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub properties: LayerProperties,
    pub mask: Option<LayerMask>,
    pub content: LayerContent,
}
impl Layer {
    pub fn raster(name: String, samples: TileSurface<Rgba8>) -> Self {
        Self {
            id: next_id(),
            name,
            properties: LayerProperties::default(),
            mask: None,
            content: LayerContent::Raster(Arc::new(samples)),
        }
    }
    pub fn group(name: String, children: Vec<Layer>) -> Self {
        Self {
            id: next_id(),
            name,
            properties: LayerProperties {
                pass_through: true,
                ..LayerProperties::default()
            },
            mask: None,
            content: LayerContent::Group(children),
        }
    }
    pub fn raster_samples(&self) -> Result<&TileSurface<Rgba8>, String> {
        match &self.content {
            LayerContent::Raster(samples) => Ok(samples),
            _ => Err("Choose a raster layer to paint".into()),
        }
    }
    pub fn raster_samples_mut(&mut self) -> Result<&mut TileSurface<Rgba8>, String> {
        match &mut self.content {
            LayerContent::Raster(samples) => Ok(Arc::make_mut(samples)),
            _ => Err("Choose a raster layer to paint".into()),
        }
    }
    pub fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(mask) = &self.mask {
            for tile in mask.samples.tiles().values() {
                visitor(TileRef::Gray(tile));
            }
        }
        match &self.content {
            LayerContent::Raster(samples) => {
                for tile in samples.tiles().values() {
                    visitor(TileRef::Rgba(tile));
                }
            }
            LayerContent::Group(children) => {
                for child in children {
                    child.visit_tiles(visitor);
                }
            }
        }
    }
    fn duplicate(&self) -> Self {
        let mut copy = self.clone();
        copy.id = next_id();
        if let LayerContent::Group(children) = &mut copy.content {
            *children = children.iter().map(Self::duplicate).collect();
        }
        copy
    }
}
pub fn find_layer(nodes: &[Layer], id: LayerId) -> Option<&Layer> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let LayerContent::Group(children) = &node.content {
            if let Some(node) = find_layer(children, id) {
                return Some(node);
            }
        }
    }
    None
}
pub fn find_layer_mut(nodes: &mut [Layer], id: LayerId) -> Option<&mut Layer> {
    for node in nodes {
        if node.id == id {
            return Some(node);
        }
        if let LayerContent::Group(children) = &mut node.content {
            if let Some(node) = find_layer_mut(children, id) {
                return Some(node);
            }
        }
    }
    None
}
fn siblings_mut(
    nodes: &mut Vec<Layer>,
    parent: Option<LayerId>,
) -> Result<&mut Vec<Layer>, String> {
    match parent {
        None => Ok(nodes),
        Some(id) => match &mut find_layer_mut(nodes, id)
            .ok_or("Group no longer exists")?
            .content
        {
            LayerContent::Group(children) => Ok(children),
            _ => Err("Destination is not a group".into()),
        },
    }
}
fn parent_of(nodes: &[Layer], id: LayerId, parent: Option<LayerId>) -> Option<Option<LayerId>> {
    for node in nodes {
        if node.id == id {
            return Some(parent);
        }
        if let LayerContent::Group(children) = &node.content {
            if let Some(parent) = parent_of(children, id, Some(node.id)) {
                return Some(parent);
            }
        }
    }
    None
}
fn take(nodes: &mut Vec<Layer>, ids: &HashSet<LayerId>, out: &mut Vec<Layer>) {
    let mut i = 0;
    while i < nodes.len() {
        if ids.contains(&nodes[i].id) {
            out.push(nodes.remove(i));
        } else {
            if let LayerContent::Group(children) = &mut nodes[i].content {
                take(children, ids, out);
            }
            i += 1;
        }
    }
}
fn normalized_selection(nodes: &[Layer], ids: &[LayerId]) -> Result<HashSet<LayerId>, String> {
    let ids: HashSet<_> = ids.iter().copied().collect();
    if ids.is_empty() || ids.iter().any(|id| find_layer(nodes, *id).is_none()) {
        return Err("Select existing layers".into());
    }
    fn walk(nodes: &[Layer], selected: &HashSet<LayerId>, roots: &mut HashSet<LayerId>) {
        for node in nodes {
            if selected.contains(&node.id) {
                roots.insert(node.id);
            } else if let LayerContent::Group(children) = &node.content {
                walk(children, selected, roots);
            }
        }
    }
    let mut roots = HashSet::new();
    walk(nodes, &ids, &mut roots);
    Ok(roots)
}
pub fn check_layer_edit(nodes: &[Layer], id: LayerId, position: bool) -> Result<(), String> {
    let node = find_layer(nodes, id).ok_or("Layer no longer exists")?;
    if node.properties.locks.all
        || if position {
            node.properties.locks.position
        } else {
            node.properties.locks.pixels
        }
    {
        return Err("This layer or its parent is locked".into());
    }
    if let Some(Some(parent)) = parent_of(nodes, id, None) {
        check_layer_edit(nodes, parent, position)?;
    }
    Ok(())
}
pub fn validate_layers(nodes: &[Layer], width: u32, height: u32) -> Result<(), String> {
    fn validate(
        nodes: &[Layer],
        w: u32,
        h: u32,
        depth: usize,
        ids: &mut HashSet<LayerId>,
    ) -> Result<(), String> {
        if depth > MAX_GROUP_DEPTH {
            return Err("Groups may be nested at most 32 levels".into());
        }
        for node in nodes {
            let p = &node.properties;
            if node.id == 0
                || node.id >= i32::MAX as u32
                || !ids.insert(node.id)
                || ids.len() > MAX_LAYERS
            {
                return Err("Invalid/duplicate layer ID or more than 512 layer/group nodes".into());
            }
            if node.name.is_empty()
                || node.name.len() > 512
                || node.name.chars().any(char::is_control)
                || [p.opacity, p.fill]
                    .into_iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            {
                return Err("Invalid layer name or opacity/fill".into());
            }
            if let Some(mask) = &node.mask {
                if (
                    mask.samples.width,
                    mask.samples.height,
                    mask.samples.default,
                ) != (w, h, Gray8(255))
                {
                    return Err("Invalid mask surface".into());
                }
            }
            match &node.content {
                LayerContent::Raster(samples) => {
                    if (samples.width, samples.height, samples.default) != (w, h, Rgba8::default())
                        || p.pass_through
                    {
                        return Err("Invalid raster surface or pass-through raster".into());
                    }
                }
                LayerContent::Group(children) => {
                    if p.pass_through && p.blend != BlendMode::Normal {
                        return Err("Pass-through groups use Normal blend metadata".into());
                    }
                    validate(children, w, h, depth + 1, ids)?;
                }
            }
        }
        Ok(())
    }
    validate(nodes, width, height, 0, &mut HashSet::new())
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum LayerAction {
    Add {
        name: String,
        parent: Option<LayerId>,
        index: usize,
        group: bool,
    },
    Delete {
        ids: Vec<LayerId>,
    },
    Duplicate {
        ids: Vec<LayerId>,
    },
    Rename {
        target: LayerId,
        name: String,
    },
    Move {
        ids: Vec<LayerId>,
        parent: Option<LayerId>,
        index: usize,
    },
    Group {
        ids: Vec<LayerId>,
        name: String,
    },
    Ungroup {
        target: LayerId,
    },
    Properties {
        target: LayerId,
        properties: LayerProperties,
    },
    MaskAdd {
        target: LayerId,
    },
    MaskDelete {
        target: LayerId,
    },
    MaskEnabled {
        target: LayerId,
        enabled: bool,
    },
}
pub struct LayerCommand {
    before: Vec<Layer>,
    after: Vec<Layer>,
    label: &'static str,
    empty: bool,
}
fn equivalent(a: &[Layer], b: &[Layer]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id
                && a.name == b.name
                && a.properties == b.properties
                && match (&a.mask, &b.mask) {
                    (None, None) => true,
                    (Some(a), Some(b)) => {
                        a.enabled == b.enabled && Arc::ptr_eq(&a.samples, &b.samples)
                    }
                    _ => false,
                }
                && match (&a.content, &b.content) {
                    (LayerContent::Raster(a), LayerContent::Raster(b)) => Arc::ptr_eq(a, b),
                    (LayerContent::Group(a), LayerContent::Group(b)) => equivalent(a, b),
                    _ => false,
                }
        })
}
impl LayerCommand {
    pub fn prepare(document: &Document, action: LayerAction) -> Result<Self, String> {
        let mut after = document.layers.clone();
        let mut empty = false;
        let label = match action {
            LayerAction::Add {
                name,
                parent,
                index,
                group,
            } => {
                if let Some(parent) = parent {
                    check_layer_edit(&after, parent, true)?;
                }
                let node = if group {
                    Layer::group(name, Vec::new())
                } else {
                    Layer::raster(name, TileSurface::new(document.width, document.height))
                };
                let nodes = siblings_mut(&mut after, parent)?;
                if index > nodes.len() {
                    return Err("Invalid layer insertion position".into());
                }
                nodes.insert(index, node);
                if group {
                    "Add group"
                } else {
                    "Add layer"
                }
            }
            LayerAction::Delete { ids } => {
                let selected = normalized_selection(&after, &ids)?;
                for id in &selected {
                    check_layer_edit(&after, *id, true)?;
                }
                take(&mut after, &selected, &mut Vec::new());
                "Delete layers"
            }
            LayerAction::Duplicate { ids } => {
                let selected = normalized_selection(&after, &ids)?;
                fn duplicate(nodes: &mut Vec<Layer>, selected: &HashSet<LayerId>) {
                    let mut i = 0;
                    while i < nodes.len() {
                        if selected.contains(&nodes[i].id) {
                            let mut copy = nodes[i].duplicate();
                            copy.name = format!("{} copy", copy.name);
                            nodes.insert(i + 1, copy);
                            i += 2;
                        } else {
                            if let LayerContent::Group(children) = &mut nodes[i].content {
                                duplicate(children, selected);
                            }
                            i += 1;
                        }
                    }
                }
                duplicate(&mut after, &selected);
                "Duplicate layers"
            }
            LayerAction::Rename { target, name } => {
                let node = find_layer_mut(&mut after, target).ok_or("Layer no longer exists")?;
                empty = node.name == name;
                node.name = name;
                "Rename layer"
            }
            LayerAction::Move { ids, parent, index } => {
                let selected = normalized_selection(&after, &ids)?;
                for id in &selected {
                    check_layer_edit(&after, *id, true)?;
                    if let Some(parent) = parent {
                        let node = find_layer(&after, *id).unwrap();
                        if *id == parent
                            || matches!(&node.content,LayerContent::Group(children) if find_layer(children,parent).is_some())
                        {
                            return Err("A group cannot contain itself".into());
                        }
                    }
                }
                if let Some(parent) = parent {
                    check_layer_edit(&after, parent, true)?;
                }
                let mut moved = Vec::new();
                take(&mut after, &selected, &mut moved);
                let nodes = siblings_mut(&mut after, parent)?;
                // Index is in the destination after selected roots are removed.
                if index > nodes.len() {
                    return Err("Invalid layer insertion position".into());
                }
                nodes.splice(index..index, moved);
                "Move layers"
            }
            LayerAction::Group { ids, name } => {
                let selected = normalized_selection(&after, &ids)?;
                let first = *selected.iter().min().unwrap();
                let parent = parent_of(&after, first, None).unwrap();
                for id in &selected {
                    check_layer_edit(&after, *id, true)?;
                    if parent_of(&after, *id, None) != Some(parent) {
                        return Err("Group layers from the same parent".into());
                    }
                }
                let nodes = siblings_mut(&mut after, parent)?;
                let index = nodes.iter().position(|n| selected.contains(&n.id)).unwrap();
                let mut children = Vec::new();
                take(nodes, &selected, &mut children);
                nodes.insert(index, Layer::group(name, children));
                "Group layers"
            }
            LayerAction::Ungroup { target } => {
                check_layer_edit(&after, target, true)?;
                let parent = parent_of(&after, target, None).ok_or("Layer no longer exists")?;
                let nodes = siblings_mut(&mut after, parent)?;
                let index = nodes.iter().position(|n| n.id == target).unwrap();
                let node = nodes.remove(index);
                if node.mask.is_some()
                    || node.properties
                        != (LayerProperties {
                            pass_through: true,
                            ..LayerProperties::default()
                        })
                {
                    return Err("Only an unmasked default pass-through group can be ungrouped without changing its appearance".into());
                }
                match node.content {
                    LayerContent::Group(children) => {
                        nodes.splice(index..index, children);
                    }
                    _ => return Err("Select a group".into()),
                }
                "Ungroup layers"
            }
            LayerAction::Properties { target, properties } => {
                let node = find_layer_mut(&mut after, target).ok_or("Layer no longer exists")?;
                if node.properties.locks.all
                    && properties.locks.all
                    && (node.properties.opacity != properties.opacity
                        || node.properties.fill != properties.fill
                        || node.properties.blend != properties.blend
                        || node.properties.pass_through != properties.pass_through
                        || node.properties.clipped != properties.clipped)
                {
                    return Err("Unlock the layer before changing its properties".into());
                }
                empty = node.properties == properties;
                node.properties = properties;
                "Layer properties"
            }
            LayerAction::MaskAdd { target } => {
                check_layer_edit(&after, target, false)?;
                let node = find_layer_mut(&mut after, target).unwrap();
                if node.mask.is_some() {
                    return Err("Layer already has a mask".into());
                }
                node.mask = Some(LayerMask {
                    enabled: true,
                    samples: Arc::new(TileSurface::with_default(
                        document.width,
                        document.height,
                        Gray8(255),
                    )),
                });
                "Add layer mask"
            }
            LayerAction::MaskDelete { target } => {
                check_layer_edit(&after, target, false)?;
                if find_layer_mut(&mut after, target)
                    .unwrap()
                    .mask
                    .take()
                    .is_none()
                {
                    return Err("Layer has no mask".into());
                }
                "Delete layer mask"
            }
            LayerAction::MaskEnabled { target, enabled } => {
                let mask = find_layer_mut(&mut after, target)
                    .ok_or("Layer no longer exists")?
                    .mask
                    .as_mut()
                    .ok_or("Layer has no mask")?;
                empty = mask.enabled == enabled;
                mask.enabled = enabled;
                "Enable/disable layer mask"
            }
        };
        validate_layers(&after, document.width, document.height)?;
        empty |= equivalent(&document.layers, &after);
        Ok(Self {
            before: document.layers.clone(),
            after,
            label,
            empty,
        })
    }
    pub fn replacing(
        document: &Document,
        after: Vec<Layer>,
        label: &'static str,
    ) -> Result<Self, String> {
        validate_layers(&after, document.width, document.height)?;
        Ok(Self {
            before: document.layers.clone(),
            after,
            label,
            empty: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{History, TileCommand};
    use proptest::prelude::*;
    fn state(d: &Document) -> String {
        fn nodes(layers: &[Layer], out: &mut String) {
            for layer in layers {
                out.push_str(&format!(
                    "{}:{}:{:?}",
                    layer.id, layer.name, layer.properties
                ));
                if let Some(mask) = &layer.mask {
                    out.push_str(&format!(
                        "mask:{}:{:?}",
                        mask.enabled,
                        mask.samples.sample(0, 0)
                    ));
                }
                match &layer.content {
                    LayerContent::Raster(samples) => {
                        out.push_str(&format!("raster:{:?}", samples.sample(0, 0)))
                    }
                    LayerContent::Group(children) => {
                        out.push('[');
                        nodes(children, out);
                        out.push(']');
                    }
                }
            }
        }
        let mut out = format!("{}:{}:{}", d.width, d.height, d.dpi);
        nodes(&d.layers, &mut out);
        out
    }
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]
        #[test]
        fn random_layer_sequences_undo_all_and_restore_exact_state(actions in prop::collection::vec((0u8..11,any::<u8>(),any::<[u8;4]>()),1..65)) {
            let mut document = Document::blank(1,1,72.0).unwrap();
            document.raster_mut().write(0,0,Rgba8([87,33,19,0]));
            let initial = state(&document);
            let mut history = History::default();
            for (kind,n,pixel) in actions {
                if kind == 0 { history.undo(&mut document); continue; }
                if kind == 1 { history.redo(&mut document); continue; }
                let ids: Vec<_> = document.layers.iter().map(|n| n.id).collect();
                let target = ids.get(n as usize % ids.len().max(1)).copied();
                let action = match (kind,target) {
                    (2,_) => Some(LayerAction::Add { name:format!("Layer {n}"),parent:None,index:document.layers.len(),group:false }),
                    (3,Some(target)) => Some(LayerAction::Delete { ids:vec![target] }),
                    (4,Some(target)) => Some(LayerAction::Duplicate { ids:vec![target] }),
                    (5,Some(target)) => Some(LayerAction::Rename { target,name:format!("Name {n}") }),
                    (6,Some(target)) => Some(LayerAction::Properties { target,properties:LayerProperties { opacity:f64::from(n)/255.0,visible:n%2==0,..document.layer(target).unwrap().properties.clone() } }),
                    (7,Some(target)) => Some(LayerAction::MaskAdd { target }),
                    (8,Some(target)) => Some(LayerAction::MaskDelete { target }),
                    (9,Some(target)) => Some(LayerAction::Group { ids:vec![target],name:format!("Group {n}") }),
                    (10,Some(target)) => {
                        if let Ok(samples) = document.layer(target).unwrap().raster_samples() {
                            let mut edited = document.clone();
                            let _ = samples;
                            edited.layer_mut(target).unwrap().raster_samples_mut().unwrap().write(0,0,Rgba8(pixel));
                            let c = TileCommand::for_layer(&document,&edited,target).unwrap();
                            history.execute(&mut document,Box::new(c),0).unwrap();
                        }
                        None
                    },
                    _ => None,
                };
                if let Some(action) = action {
                    if let Ok(command) = LayerCommand::prepare(&document,action) { history.execute(&mut document,Box::new(command),0).unwrap(); }
                }
                validate_layers(&document.layers,1,1).unwrap();
            }
            let final_state = state(&document);
            let cursor = history.depths().0;
            while history.undo(&mut document) {}
            prop_assert_eq!(state(&document),initial);
            history.jump(&mut document,cursor).unwrap();
            prop_assert_eq!(state(&document),final_state);
        }
    }
    fn execute(d: &mut Document, h: &mut History, action: LayerAction) {
        let before = state(d);
        let c = LayerCommand::prepare(d, action).unwrap();
        assert!(h.execute(d, Box::new(c), 0).unwrap());
        let after = state(d);
        assert!(h.undo(d));
        assert_eq!(state(d), before);
        assert!(h.redo(d));
        assert_eq!(state(d), after);
    }
    #[test]
    fn metadata_group_mask_duplicate_move_delete_commands_undo_exactly() {
        let mut d = Document::blank(1, 1, 299.123).unwrap();
        d.raster_mut().write(0, 0, Rgba8([87, 33, 19, 0]));
        let initial = state(&d);
        let first = d.layers[0].id;
        let mut h = History::default();
        execute(&mut d, &mut h, LayerAction::Duplicate { ids: vec![first] });
        let second = d.layers[1].id;
        assert_eq!(d.bytes(), 262144); // Duplicate shares the immutable source surface.
        execute(
            &mut d,
            &mut h,
            LayerAction::Rename {
                target: second,
                name: "Copy renamed".into(),
            },
        );
        execute(&mut d, &mut h, LayerAction::MaskAdd { target: second });
        assert_eq!(
            d.layer(second)
                .unwrap()
                .mask
                .as_ref()
                .unwrap()
                .samples
                .bytes(),
            0
        );
        assert_eq!(
            d.layer(second)
                .unwrap()
                .mask
                .as_ref()
                .unwrap()
                .samples
                .sample(0, 0),
            Gray8(255)
        );
        execute(
            &mut d,
            &mut h,
            LayerAction::MaskEnabled {
                target: second,
                enabled: false,
            },
        );
        execute(&mut d, &mut h, LayerAction::MaskDelete { target: second });
        execute(
            &mut d,
            &mut h,
            LayerAction::Group {
                ids: vec![first, second],
                name: "Both".into(),
            },
        );
        let group = d.layers[0].id;
        assert!(d.layer(group).unwrap().properties.pass_through);
        execute(
            &mut d,
            &mut h,
            LayerAction::Move {
                ids: vec![second],
                parent: Some(group),
                index: 0,
            },
        );
        execute(&mut d, &mut h, LayerAction::Ungroup { target: group });
        let p = LayerProperties {
            opacity: 0.3,
            fill: 0.6,
            blend: BlendMode::Multiply,
            visible: false,
            ..LayerProperties::default()
        };
        execute(
            &mut d,
            &mut h,
            LayerAction::Properties {
                target: second,
                properties: p,
            },
        );
        execute(
            &mut d,
            &mut h,
            LayerAction::Delete {
                ids: vec![first, second],
            },
        );
        assert!(d.layers.is_empty());
        while h.undo(&mut d) {}
        assert_eq!(state(&d), initial);
        assert_eq!(d.raster().sample(0, 0), Rgba8([87, 33, 19, 0]));
    }
    #[test]
    fn invalid_moves_locked_ancestors_and_noops_preserve_document_and_redo() {
        let mut d = Document::blank(1, 1, 72.0).unwrap();
        let first = d.layers[0].id;
        let mut h = History::default();
        execute(
            &mut d,
            &mut h,
            LayerAction::Group {
                ids: vec![first],
                name: "G".into(),
            },
        );
        let group = d.layers[0].id;
        let baseline = state(&d);
        assert!(LayerCommand::prepare(
            &d,
            LayerAction::Move {
                ids: vec![group],
                parent: Some(first),
                index: 0
            }
        )
        .is_err());
        assert_eq!(state(&d), baseline);
        let no_op = LayerCommand::prepare(
            &d,
            LayerAction::Move {
                ids: vec![first],
                parent: Some(group),
                index: 0,
            },
        )
        .unwrap();
        assert!(!h.execute(&mut d, Box::new(no_op), 0).unwrap());
        d.layers[0].properties.locks.position = true;
        assert!(LayerCommand::prepare(&d, LayerAction::Delete { ids: vec![first] }).is_err());
        d.layers[0].properties.locks.position = false;
        d.layers[0].properties.locks.all = true;
        assert!(check_layer_edit(&d.layers, first, false).is_err());
        d.layers[0].properties.locks.all = false;
        h.undo(&mut d);
        let no_op = LayerCommand::prepare(
            &d,
            LayerAction::Rename {
                target: first,
                name: d.layers[0].name.clone(),
            },
        )
        .unwrap();
        assert!(!h.execute(&mut d, Box::new(no_op), 0).unwrap());
        assert_eq!(h.depths(), (0, 1));
    }
    #[test]
    fn tile_commands_follow_stable_layer_ids_through_reordering_and_gray_history_counts_once() {
        let mut d = Document::blank(1, 1, 72.0).unwrap();
        let first = d.layers[0].id;
        let mut h = History::default();
        let mut edited = d.clone();
        edited.raster_mut().write(0, 0, Rgba8([201, 3, 7, 1]));
        let tile_command = TileCommand::between(&d, &edited).unwrap();
        h.execute(&mut d, Box::new(tile_command), 0).unwrap();
        execute(&mut d, &mut h, LayerAction::Duplicate { ids: vec![first] });
        execute(
            &mut d,
            &mut h,
            LayerAction::Move {
                ids: vec![first],
                parent: None,
                index: 1,
            },
        );
        let second = d.layers[0].id;
        execute(&mut d, &mut h, LayerAction::MaskAdd { target: second });
        let mut after = d.layers.clone();
        Arc::make_mut(
            &mut find_layer_mut(&mut after, second)
                .unwrap()
                .mask
                .as_mut()
                .unwrap()
                .samples,
        )
        .write(0, 0, Gray8(127));
        let command = LayerCommand::replacing(&d, after, "Paint mask").unwrap();
        h.execute(&mut d, Box::new(command), 0).unwrap();
        assert_eq!(d.bytes(), 262144 + 65536);
        h.undo(&mut d);
        assert_eq!(d.bytes(), 262144);
        assert_eq!(h.bytes(&d), 65536);
        while h.undo(&mut d) {}
        assert_eq!(d.layers[0].id, first);
        assert_eq!(d.raster().sample(0, 0), Rgba8::default());
    }
}
impl Command for LayerCommand {
    fn label(&self) -> &str {
        self.label
    }
    fn apply(&self, document: &mut Document) {
        document.layers = self.after.clone();
    }
    fn revert(&self, document: &mut Document) {
        document.layers = self.before.clone();
    }
    fn visit_tiles(&self, visitor: &mut dyn FnMut(TileRef<'_>)) {
        for node in self.before.iter().chain(&self.after) {
            node.visit_tiles(visitor);
        }
    }
    fn is_empty(&self) -> bool {
        self.empty
    }
}

/// Borrowed generations of either sample type share the same identity space.
#[derive(Clone, Copy)]
pub enum TileRef<'a> {
    Rgba(&'a TileHandle<Rgba8>),
    Gray(&'a TileHandle<Gray8>),
}
impl TileRef<'_> {
    pub fn id(self) -> u64 {
        match self {
            Self::Rgba(t) => t.id(),
            Self::Gray(t) => t.id(),
        }
    }
    pub fn bytes(self) -> u64 {
        match self {
            Self::Rgba(t) => t.resident_bytes(),
            Self::Gray(t) => t.resident_bytes(),
        }
    }
}
