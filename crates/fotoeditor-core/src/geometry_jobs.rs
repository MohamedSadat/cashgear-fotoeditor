//! Atomic, pageable whole-document geometry. Every surface uses one source.
use crate::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum GeometryAction {
    ImageSize {
        width: u32,
        height: u32,
        dpi: f64,
        #[serde(default)]
        interpolation: Interpolation,
    },
    CanvasSize {
        width: u32,
        height: u32,
        anchor: [f64; 2],
    },
    CanvasRotate {
        degrees: f64,
        #[serde(default)]
        interpolation: Interpolation,
    },
    CanvasFlip {
        horizontal: bool,
    },
}
#[derive(Clone, Copy)]
enum Destination {
    Raster(LayerId),
    Mask(LayerId),
    Selection,
}
enum Work {
    Rgba(SurfaceResampleJob<Rgba8>),
    Gray(SurfaceResampleJob<Gray8>),
}
impl Work {
    fn done(&self) -> bool {
        match self {
            Self::Rgba(j) => j.done(),
            Self::Gray(j) => j.done(),
        }
    }
    fn step(&mut self) -> Result<bool, String> {
        match self {
            Self::Rgba(j) => j.step(),
            Self::Gray(j) => j.step(),
        }
    }
    fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        match self {
            Self::Rgba(j) => j.visit_tiles(v),
            Self::Gray(j) => j.visit_tiles(v),
        }
    }
    fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        match self {
            Self::Rgba(j) => j.needed(v),
            Self::Gray(j) => j.needed(v),
        }
    }
    fn scratch_bytes(&self) -> u64 {
        match self {
            Self::Rgba(j) => {
                if j.identity {
                    0
                } else {
                    RESAMPLE_SCRATCH
                }
            }
            Self::Gray(j) => {
                if j.identity {
                    0
                } else {
                    RESAMPLE_SCRATCH
                }
            }
        }
    }
}
fn full(d: &Document) -> CanvasRect {
    CanvasRect {
        x: 0,
        y: 0,
        width: d.width,
        height: d.height,
    }
}
fn rounded(v: f64) -> f64 {
    if (v - v.round()).abs() < 1e-8 {
        v.round()
    } else {
        v
    }
}
fn enclosing(r: TransformRect) -> [f64; 4] {
    [
        rounded(r.x).floor(),
        rounded(r.y).floor(),
        rounded(r.x + r.width).ceil(),
        rounded(r.y + r.height).ceil(),
    ]
}
fn set_canvas(d: &mut Document, r: CanvasRect) {
    d.canvas = (r != full(d)).then_some(r);
}
fn targets(nodes: &[Layer], out: &mut Vec<Destination>) {
    for l in nodes {
        if l.mask.is_some() {
            out.push(Destination::Mask(l.id));
        }
        match &l.content {
            LayerContent::Raster(_) => out.push(Destination::Raster(l.id)),
            LayerContent::Group(c) => targets(c, out),
            LayerContent::Adjustment(_) => {}
        }
    }
}
/// Only metadata is published by finish's command. Pending surfaces are retained
/// by this job and visited for shared CPU residency and durable paging.
pub struct GeometryJob {
    pub source: Arc<Document>,
    pub output: Document,
    matrix: TransformMatrix,
    interpolation: Interpolation,
    normalize: bool,
    targets: Vec<Destination>,
    index: usize,
    work: Option<Work>,
    label: &'static str,
    available: u64,
}
impl GeometryJob {
    pub fn new(
        source: Arc<Document>,
        action: GeometryAction,
        available: u64,
    ) -> Result<Self, String> {
        source.validate_geometry()?;
        validate_layers(&source.layers, source.width, source.height)?;
        let mut output = (*source).clone();
        let c = source.canvas_rect();
        let (matrix, interpolation, normalize, label) = match action {
            GeometryAction::ImageSize {
                width,
                height,
                dpi,
                interpolation,
            } => {
                validate_size(width, height, dpi)?;
                let sx = width as f64 / c.width as f64;
                let sy = height as f64 / c.height as f64;
                let x = rounded(c.x as f64 * sx).ceil();
                let y = rounded(c.y as f64 * sy).ceil();
                let delta = [x - c.x as f64 * sx, y - c.y as f64 * sy];
                let m = TransformMatrix::scale(sx, sy)
                    .then(TransformMatrix::translation(delta[0], delta[1]));
                output.width = rounded(source.width as f64 * sx + delta[0]).ceil() as u32;
                output.height = rounded(source.height as f64 * sy + delta[1]).ceil() as u32;
                output.dpi = dpi;
                set_canvas(
                    &mut output,
                    CanvasRect {
                        x: x as u32,
                        y: y as u32,
                        width,
                        height,
                    },
                );
                for g in &mut output.guides {
                    g.position = match g.axis {
                        GuideAxis::Vertical => g.position * sx + delta[0],
                        GuideAxis::Horizontal => g.position * sy + delta[1],
                    };
                }
                (m, interpolation, true, "Image size")
            }
            GeometryAction::CanvasSize {
                width,
                height,
                anchor,
            } => {
                validate_size(width, height, source.dpi)?;
                if anchor
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                {
                    return Err("Canvas anchor must be within [0,1]".into());
                }
                let x = c.x as i64 + ((c.width as f64 - width as f64) * anchor[0]).round() as i64;
                let y = c.y as i64 + ((c.height as f64 - height as f64) * anchor[1]).round() as i64;
                let left = x.min(0);
                let top = y.min(0);
                output.width = (source.width as i64)
                    .max(x + width as i64)
                    .checked_sub(left)
                    .ok_or("Canvas width overflow")?
                    .try_into()
                    .map_err(|_| "Canvas width overflow")?;
                output.height = (source.height as i64)
                    .max(y + height as i64)
                    .checked_sub(top)
                    .ok_or("Canvas height overflow")?
                    .try_into()
                    .map_err(|_| "Canvas height overflow")?;
                set_canvas(
                    &mut output,
                    CanvasRect {
                        x: (x - left) as u32,
                        y: (y - top) as u32,
                        width,
                        height,
                    },
                );
                for g in &mut output.guides {
                    g.position -= match g.axis {
                        GuideAxis::Vertical => left as f64,
                        GuideAxis::Horizontal => top as f64,
                    };
                }
                (
                    TransformMatrix::translation(-left as f64, -top as f64),
                    Interpolation::Nearest,
                    false,
                    "Canvas size",
                )
            }
            GeometryAction::CanvasRotate {
                degrees,
                interpolation,
            } => {
                if !degrees.is_finite() {
                    return Err("Canvas rotation must be finite".into());
                }
                let m = TransformMatrix::rotation(
                    degrees,
                    [
                        c.x as f64 + c.width as f64 / 2.0,
                        c.y as f64 + c.height as f64 / 2.0,
                    ],
                );
                let [left, top, right, bottom] = enclosing(m.bounds(full(&source).geometry())?);
                output.width = (right - left) as u32;
                output.height = (bottom - top) as u32;
                let m = m.then(TransformMatrix::translation(-left, -top));
                let [x, y, right, bottom] = enclosing(m.bounds(c.geometry())?);
                let new_canvas = CanvasRect {
                    x: x as u32,
                    y: y as u32,
                    width: (right - x) as u32,
                    height: (bottom - y) as u32,
                };
                set_canvas(&mut output, new_canvas);
                let quarter = (degrees / 90.0).round();
                if (degrees - quarter * 90.0).abs() < 1e-8 {
                    for g in &mut output.guides {
                        let p = match g.axis {
                            GuideAxis::Vertical => [g.position, c.y as f64],
                            GuideAxis::Horizontal => [c.x as f64, g.position],
                        };
                        let p = m.map(p)?;
                        if (quarter as i64).rem_euclid(2) == 1 {
                            g.axis = match g.axis {
                                GuideAxis::Horizontal => GuideAxis::Vertical,
                                GuideAxis::Vertical => GuideAxis::Horizontal,
                            };
                        }
                        g.position = match g.axis {
                            GuideAxis::Vertical => p[0],
                            GuideAxis::Horizontal => p[1],
                        };
                    }
                } else {
                    // Axis-aligned authoring guides retain their canvas-relative
                    // offsets when an arbitrary rotation creates angled edges.
                    for g in &mut output.guides {
                        g.position += match g.axis {
                            GuideAxis::Vertical => new_canvas.x as f64 - c.x as f64,
                            GuideAxis::Horizontal => new_canvas.y as f64 - c.y as f64,
                        };
                        let limit = match g.axis {
                            GuideAxis::Vertical => output.width,
                            GuideAxis::Horizontal => output.height,
                        };
                        g.position = g.position.clamp(0.0, limit as f64);
                    }
                }
                (m, interpolation, false, "Rotate canvas")
            }
            GeometryAction::CanvasFlip { horizontal } => {
                let m = if horizontal {
                    TransformMatrix::scale(-1.0, 1.0).then(TransformMatrix::translation(
                        (2 * c.x + c.width) as f64,
                        0.0,
                    ))
                } else {
                    TransformMatrix::scale(1.0, -1.0).then(TransformMatrix::translation(
                        0.0,
                        (2 * c.y + c.height) as f64,
                    ))
                };
                let [left, top, right, bottom] = enclosing(m.bounds(full(&source).geometry())?);
                output.width = (right - left) as u32;
                output.height = (bottom - top) as u32;
                let m = m.then(TransformMatrix::translation(-left, -top));
                let [x, y, right, bottom] = enclosing(m.bounds(c.geometry())?);
                set_canvas(
                    &mut output,
                    CanvasRect {
                        x: x as u32,
                        y: y as u32,
                        width: (right - x) as u32,
                        height: (bottom - y) as u32,
                    },
                );
                for g in &mut output.guides {
                    let p = match g.axis {
                        GuideAxis::Vertical => m.map([g.position, 0.0])?[0],
                        GuideAxis::Horizontal => m.map([0.0, g.position])?[1],
                    };
                    g.position = p;
                }
                (m, Interpolation::Nearest, false, "Flip canvas")
            }
        };
        output.validate_geometry()?;
        matrix.validate_for(full(&source).geometry())?;
        let mut todo = vec![];
        targets(&source.layers, &mut todo);
        if source.selection.is_some() {
            todo.push(Destination::Selection);
        }
        let mut job = Self {
            source,
            output,
            matrix,
            interpolation,
            normalize,
            targets: todo,
            index: 0,
            work: None,
            label,
            available,
        };
        job.prepare()?;
        Ok(job)
    }
    pub fn done(&self) -> bool {
        self.index == self.targets.len()
    }
    pub fn progress(&self) -> f64 {
        if self.targets.is_empty() {
            1.0
        } else {
            self.index as f64 / self.targets.len() as f64
        }
    }
    pub fn scratch_bytes(&self) -> u64 {
        self.work.as_ref().map_or(0, Work::scratch_bytes)
    }
    pub fn visit_tiles(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        self.source.visit_tiles(v);
        self.output.visit_tiles(v);
        if let Some(w) = &self.work {
            w.visit_tiles(v);
        }
    }
    pub fn needed(&self, v: &mut dyn FnMut(TileRef<'_>)) {
        if let Some(w) = &self.work {
            w.needed(v);
        }
    }
    fn prepare(&mut self) -> Result<(), String> {
        if self.done() {
            return Ok(());
        }
        let (w, h) = (self.output.width, self.output.height);
        // Candidate generations are budgeted independently from the source.
        let mut ids = std::collections::HashSet::new();
        self.source.visit_tiles(&mut |t| {
            ids.insert(t.id());
        });
        let mut used = 0;
        self.output.visit_tiles(&mut |t| {
            if ids.insert(t.id()) {
                used += t.bytes();
            }
        });
        let available = self.available.saturating_sub(used);
        self.work = Some(match self.targets[self.index] {
            Destination::Raster(id) => {
                let LayerContent::Raster(s) = &self.source.layer(id)?.content else {
                    return Err("Geometry raster changed".into());
                };
                Work::Rgba(SurfaceResampleJob::new(
                    s.clone(),
                    w,
                    h,
                    self.matrix,
                    self.interpolation,
                    self.normalize,
                    available,
                )?)
            }
            Destination::Mask(id) => {
                let s = &self
                    .source
                    .layer(id)?
                    .mask
                    .as_ref()
                    .ok_or("Geometry mask changed")?
                    .samples;
                Work::Gray(SurfaceResampleJob::new(
                    s.clone(),
                    w,
                    h,
                    self.matrix,
                    self.interpolation,
                    self.normalize,
                    available,
                )?)
            }
            Destination::Selection => Work::Gray(SurfaceResampleJob::with_outside(
                self.source
                    .selection
                    .as_ref()
                    .ok_or("Geometry selection changed")?
                    .clone(),
                w,
                h,
                self.matrix,
                self.interpolation,
                self.normalize,
                available,
                Gray8(0),
            )?),
        });
        Ok(())
    }
    pub fn step(&mut self) -> Result<bool, String> {
        if self.done() {
            return Ok(true);
        }
        let w = self.work.as_mut().ok_or("No geometry surface")?;
        if !w.done() && !w.step()? {
            return Ok(false);
        }
        match (self.targets[self.index], self.work.take().unwrap()) {
            (Destination::Raster(id), Work::Rgba(j)) => {
                if !j.identity {
                    self.output.layer_mut(id)?.content =
                        LayerContent::Raster(Arc::new(j.finish()?));
                }
            }
            (Destination::Mask(id), Work::Gray(j)) => {
                if !j.identity {
                    self.output.layer_mut(id)?.mask.as_mut().unwrap().samples =
                        Arc::new(j.finish()?);
                }
            }
            (Destination::Selection, Work::Gray(j)) => {
                if !j.identity {
                    self.output.selection = Some(Arc::new(j.finish()?));
                }
            }
            _ => return Err("Geometry surface type changed".into()),
        }
        self.index += 1;
        self.prepare()?;
        Ok(self.done())
    }
    pub fn finish(self) -> Result<DocumentCommand, String> {
        if !self.done() {
            return Err("Geometry operation is incomplete".into());
        }
        DocumentCommand::between(&self.source, self.output, self.label)
    }
}
