//! Immutable transform source and validated absolute/relative handle updates.
use crate::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum TransformUpdate {
    Matrix { matrix: TransformMatrix },
    Compose { matrix: TransformMatrix },
    Quad { points: [[f64; 2]; 4] },
    Interpolation { interpolation: Interpolation },
}
pub struct TransformSession {
    pub source: Arc<Document>,
    pub target: LayerId,
    pub selected: bool,
    pub bounds: TransformRect,
    pub matrix: TransformMatrix,
    pub interpolation: Interpolation,
    bounds_ready: bool,
    empty: bool,
}
impl TransformSession {
    pub fn new(source: Arc<Document>, target: LayerId, selected: bool) -> Result<Self, String> {
        source.validate_geometry()?;
        check_layer_edit(&source.layers, target, true)?;
        source.layer(target)?.raster_samples()?;
        let selected = selected && source.selection.is_some();
        if selected {
            check_layer_edit(&source.layers, target, false)?;
            if source.layer(target)?.properties.locks.alpha {
                return Err("Unlock alpha to transform selected pixels".into());
            }
        }
        let bounds = if selected {
            source.canvas_rect().geometry()
        } else {
            layer_transform_bounds(&source, target)?
        };
        Ok(Self {
            source,
            target,
            selected,
            bounds,
            matrix: TransformMatrix::IDENTITY,
            interpolation: Interpolation::default(),
            bounds_ready: false,
            empty: false,
        })
    }
    pub fn quad(&self) -> Result<[[f64; 2]; 4], String> {
        let mut points = self.bounds.corners();
        for p in &mut points {
            *p = self.matrix.map(*p)?;
        }
        Ok(points)
    }
    pub fn update(&mut self, update: TransformUpdate) -> Result<(), String> {
        let matrix = match update {
            TransformUpdate::Matrix { matrix } => matrix,
            TransformUpdate::Compose { matrix } => self.matrix.then(matrix),
            TransformUpdate::Quad { points } => TransformMatrix::from_quad(self.bounds, points)?,
            TransformUpdate::Interpolation { interpolation } => {
                self.interpolation = interpolation;
                return Ok(());
            }
        };
        matrix.validate_for(self.bounds)?;
        self.matrix = matrix;
        Ok(())
    }
    pub fn job(&self, available: u64) -> Result<GeometryJob, String> {
        if self.empty {
            return GeometryJob::new(
                self.source.clone(),
                GeometryAction::CanvasRotate {
                    degrees: 0.0,
                    interpolation: self.interpolation,
                },
                available,
            );
        }
        GeometryJob::with_domain(
            self.source.clone(),
            GeometryAction::LayerTransform {
                target: self.target,
                matrix: self.matrix,
                interpolation: self.interpolation,
                selection: self.selected,
            },
            available,
            Some(self.bounds),
        )
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.source.visit_tiles(v);
    }
    pub fn bounds_ready(&self) -> bool {
        self.bounds_ready
    }
    pub(crate) fn accept_bounds(&mut self, bounds: Option<TransformRect>) {
        self.empty = bounds.is_none();
        if let Some(b) = bounds {
            self.bounds = b;
        }
        self.bounds_ready = true;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accumulated_rotation_uses_one_source_and_identity_preserves_samples_and_redo() {
        let mut d = Document::blank(513, 259, 72.0).unwrap();
        d.raster_mut().write(256, 200, Rgba8([87, 33, 19, 0]));
        let target = d.layers[0].id;
        let source = Arc::new(d.clone());
        let original = source.raster().tiles()[&(1, 0)].id();
        for selected in [false, true] {
            let mut s = TransformSession::new(source.clone(), target, selected).unwrap();
            assert_eq!(s.interpolation, Interpolation::Lanczos3);
            s.update(TransformUpdate::Compose {
                matrix: TransformMatrix::rotation(10.0, [256.5, 129.5]),
            })
            .unwrap();
            s.update(TransformUpdate::Compose {
                matrix: TransformMatrix::rotation(-10.0, [256.5, 129.5]),
            })
            .unwrap();
            assert_eq!(s.source.raster().tiles()[&(1, 0)].id(), original);
            let mut j = s.job(CPU_BUDGET).unwrap();
            while !j.step().unwrap() {}
            let c = j.finish().unwrap();
            assert!(c.is_empty());
            c.apply(&mut d);
            assert_eq!(d.raster().tiles()[&(1, 0)].id(), original);
            assert_eq!(d.raster().sample(256, 200), Rgba8([87, 33, 19, 0]));
        }
    }
    #[test]
    fn crossed_or_singular_handles_and_locked_ancestors_are_rejected_without_changing_session() {
        let mut d = Document::blank(32, 24, 72.0).unwrap();
        let target = d.layers[0].id;
        let mut s = TransformSession::new(Arc::new(d.clone()), target, false).unwrap();
        let before = s.quad().unwrap();
        let mut quad = before;
        quad.swap(1, 2);
        assert!(s.update(TransformUpdate::Quad { points: quad }).is_err());
        assert!(s
            .update(TransformUpdate::Matrix {
                matrix: TransformMatrix::scale(0.0, 1.0)
            })
            .is_err());
        assert_eq!(s.quad().unwrap(), before);
        let child = d.layers.remove(0);
        d.layers.push(Layer::group("Locked".into(), vec![child]));
        d.layers[0].properties.locks.position = true;
        assert!(TransformSession::new(Arc::new(d), target, false).is_err());
    }
}
