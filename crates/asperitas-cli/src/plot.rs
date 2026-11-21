//! A minimal RGB pixel canvas and PNG encoder shared by the plotting commands. Everything is
//! integer pixels with no anti-aliasing, so the encoded bytes are identical between runs.

/// Width of every plot in pixels.
pub(crate) const PLOT_W: usize = 960;

pub(crate) const GRID: [u8; 3] = [215, 215, 215];
pub(crate) const AXIS: [u8; 3] = [120, 120, 120];

pub(crate) struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    pub(crate) fn new(h: usize) -> Self {
        Self {
            px: vec![255; PLOT_W * h * 3],
        }
    }
    pub(crate) fn set(&mut self, x: usize, y: usize, rgb: [u8; 3]) {
        let i = (y * PLOT_W + x) * 3;
        if x < PLOT_W && i + 2 < self.px.len() {
            self.px[i..i + 3].copy_from_slice(&rgb);
        }
    }
    pub(crate) fn vline(&mut self, x: usize, y0: usize, y1: usize, rgb: [u8; 3]) {
        for y in y0.min(y1)..=y0.max(y1) {
            self.set(x, y, rgb);
        }
    }
    pub(crate) fn hline(&mut self, y: usize, rgb: [u8; 3]) {
        for x in 0..PLOT_W {
            self.set(x, y, rgb);
        }
    }
    /// A 2x2 dot with its top-left at `(x, y)`.
    pub(crate) fn dot(&mut self, x: usize, y: usize, rgb: [u8; 3]) {
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            self.set(x + dx, y + dy, rgb);
        }
    }
    /// Encode as an 8-bit RGB PNG.
    pub(crate) fn encode_png(&self) -> Result<Vec<u8>, String> {
        let h = self.px.len() / (PLOT_W * 3);
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, PLOT_W as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().map_err(|e| format!("png: {e}"))?;
            w.write_image_data(&self.px)
                .map_err(|e| format!("png: {e}"))?;
        }
        Ok(out)
    }
}
