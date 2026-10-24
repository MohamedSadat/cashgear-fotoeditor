#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub width: f64,
    pub height: f64,
    pub document_width: f64,
    pub document_height: f64,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub rotation: f64,
    pub flipped: bool,
}
impl Default for View {
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            document_width: 1.0,
            document_height: 1.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            rotation: 0.0,
            flipped: false,
        }
    }
}
impl View {
    pub fn document_to_screen(&self, x: f64, y: f64) -> (f64, f64) {
        let x = (x - self.document_width / 2.0) * if self.flipped { -1.0 } else { 1.0 };
        let y = y - self.document_height / 2.0;
        let (s, c) = self.rotation.sin_cos();
        (
            self.width / 2.0 + self.pan_x + (x * c - y * s) * self.zoom,
            self.height / 2.0 + self.pan_y + (x * s + y * c) * self.zoom,
        )
    }
    pub fn screen_to_document(&self, x: f64, y: f64) -> (f64, f64) {
        let x = (x - self.width / 2.0 - self.pan_x) / self.zoom;
        let y = (y - self.height / 2.0 - self.pan_y) / self.zoom;
        let (s, c) = self.rotation.sin_cos();
        (
            (x * c + y * s) * if self.flipped { -1.0 } else { 1.0 } + self.document_width / 2.0,
            -x * s + y * c + self.document_height / 2.0,
        )
    }
    pub fn zoom_at(&mut self, zoom: f64, x: f64, y: f64) {
        let p = self.screen_to_document(x, y);
        self.zoom = zoom.clamp(0.01, 32.0);
        let q = self.document_to_screen(p.0, p.1);
        self.pan_x += x - q.0;
        self.pan_y += y - q.1;
    }
    pub fn fit(&mut self) {
        let (s, c) = self.rotation.sin_cos();
        let w = self.document_width * c.abs() + self.document_height * s.abs();
        let h = self.document_width * s.abs() + self.document_height * c.abs();
        self.zoom = ((self.width - 48.0).max(1.0) / w)
            .min((self.height - 48.0).max(1.0) / h)
            .clamp(0.01, 32.0);
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }
    pub fn visible(&self, x: u32, y: u32, w: u32, h: u32) -> bool {
        let points = [
            self.screen_to_document(0.0, 0.0),
            self.screen_to_document(self.width, 0.0),
            self.screen_to_document(0.0, self.height),
            self.screen_to_document(self.width, self.height),
        ];
        let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        (x as f64) < max_x
            && ((x + w) as f64) > min_x
            && (y as f64) < max_y
            && ((y + h) as f64) > min_y
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transforms_round_trip_with_tolerance_and_anchor() {
        for zoom in [0.01, 0.1, 0.499, 1.0, 8.0, 32.0] {
            for rotation in [-3.1, -1.57, 0.0, 0.43, 3.13] {
                for flipped in [false, true] {
                    let mut v = View {
                        width: 1273.0,
                        height: 713.0,
                        document_width: 6000.0,
                        document_height: 4000.0,
                        zoom,
                        rotation,
                        flipped,
                        pan_x: 73.2,
                        pan_y: -167.7,
                    };
                    for (x, y) in [(0.0, 0.0), (5999.0, 3999.0), (3012.25, 1679.125)] {
                        let s = v.document_to_screen(x, y);
                        let p = v.screen_to_document(s.0, s.1);
                        assert!((x - p.0).abs() < 1e-8 && (y - p.1).abs() < 1e-8);
                    }
                    let before = v.screen_to_document(123.4, 567.8);
                    v.zoom_at(1.7, 123.4, 567.8);
                    let after = v.screen_to_document(123.4, 567.8);
                    assert!((before.0 - after.0).abs() < 1e-8 && (before.1 - after.1).abs() < 1e-8);
                }
            }
        }
    }
}
