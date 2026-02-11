use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Color palette constants
// ---------------------------------------------------------------------------
const PALETTE_FIRE: u8 = 0;
const PALETTE_ELECTRIC: u8 = 1;
const PALETTE_BW: u8 = 2;
const PALETTE_OCEAN: u8 = 3;

const MAX_ITER: u32 = 256;

// ---------------------------------------------------------------------------
// Fractal — the complete state for the Mandelbrot explorer.
//
// All complex-plane coordinates use f64 (double precision) to support deep
// zooming without visible pixelation artifacts.
//
// `image_buffer` is an RGBA pixel buffer (4 bytes per pixel) that lives in
// WASM linear memory.  The JS side obtains a pointer + length and builds a
// typed-array *view* directly — zero-copy rendering.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct Fractal {
    width: u32,
    height: u32,
    center_x: f64,
    center_y: f64,
    scale: f64, // complex-plane units per pixel
    palette: u8,
    max_iter: u32,
    image_buffer: Vec<u8>,
}

#[wasm_bindgen]
impl Fractal {
    // -- Constructor --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Fractal {
        let size = (width as usize) * (height as usize) * 4;
        Fractal {
            width,
            height,
            center_x: -0.5, // default view: centered on the main cardioid
            center_y: 0.0,
            scale: 3.5 / width as f64, // fit the full set horizontally
            palette: PALETTE_FIRE,
            max_iter: MAX_ITER,
            image_buffer: vec![0; size],
        }
    }

    // -- Resize -------------------------------------------------------------

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        self.width = new_width;
        self.height = new_height;
        let size = (new_width as usize) * (new_height as usize) * 4;
        self.image_buffer.resize(size, 0);
    }

    // -- Pan ----------------------------------------------------------------
    //
    // Translate a screen-space pixel delta into a complex-plane movement.
    // `dx_pixels` and `dy_pixels` are how many pixels the user dragged.

    pub fn pan(&mut self, dx_pixels: f64, dy_pixels: f64) {
        self.center_x -= dx_pixels * self.scale;
        self.center_y -= dy_pixels * self.scale;
    }

    // -- Zoom ---------------------------------------------------------------
    //
    // `factor` > 1 zooms in, < 1 zooms out.
    // The zoom is anchored so that the complex-plane point under
    // (screen_x, screen_y) stays fixed after the zoom.

    pub fn zoom(&mut self, factor: f64, screen_x: f64, screen_y: f64) {
        // Convert screen coords to complex-plane coords *before* zoom.
        let half_w = self.width as f64 / 2.0;
        let half_h = self.height as f64 / 2.0;
        let before_re = self.center_x + (screen_x - half_w) * self.scale;
        let before_im = self.center_y + (screen_y - half_h) * self.scale;

        // Apply zoom to scale.
        self.scale /= factor;

        // Re-derive center so the point under the cursor hasn't moved.
        self.center_x = before_re - (screen_x - half_w) * self.scale;
        self.center_y = before_im - (screen_y - half_h) * self.scale;
    }

    // -- Palette switching --------------------------------------------------

    pub fn set_palette(&mut self, palette: u8) {
        self.palette = palette;
    }

    pub fn palette(&self) -> u8 {
        self.palette
    }

    // -- Max iteration control ----------------------------------------------

    pub fn set_max_iter(&mut self, max_iter: u32) {
        self.max_iter = max_iter;
    }

    pub fn max_iter(&self) -> u32 {
        self.max_iter
    }

    // -- Coordinate info for UI display ------------------------------------

    pub fn center_x(&self) -> f64 {
        self.center_x
    }

    pub fn center_y(&self) -> f64 {
        self.center_y
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    // -- Render -------------------------------------------------------------
    //
    // Iterate over every pixel, compute the Mandelbrot escape time, map the
    // result to an RGBA colour, and write it to `image_buffer`.

    pub fn render(&mut self) {
        let w = self.width;
        let h = self.height;
        let half_w = w as f64 / 2.0;
        let half_h = h as f64 / 2.0;
        let scale = self.scale;
        let cx = self.center_x;
        let cy = self.center_y;
        let max_iter = self.max_iter;
        let palette = self.palette;

        for py in 0..h {
            let c_im = cy + (py as f64 - half_h) * scale;
            let row_offset = (py * w) as usize * 4;

            for px in 0..w {
                let c_re = cx + (px as f64 - half_w) * scale;

                let iter = escape_time(c_re, c_im, max_iter);

                let (r, g, b) = if iter == max_iter {
                    (0u8, 0u8, 0u8) // inside the set → black
                } else {
                    map_colour(iter, max_iter, palette)
                };

                let offset = row_offset + (px as usize) * 4;
                self.image_buffer[offset] = r;
                self.image_buffer[offset + 1] = g;
                self.image_buffer[offset + 2] = b;
                self.image_buffer[offset + 3] = 255;
            }
        }
    }

    // -- Shared-memory pointer (zero-copy rendering) -----------------------

    pub fn buffer_ptr(&self) -> *const u8 {
        self.image_buffer.as_ptr()
    }

    pub fn buffer_len(&self) -> usize {
        self.image_buffer.len()
    }

    // -- Reset to default view ---------------------------------------------

    pub fn reset(&mut self) {
        self.center_x = -0.5;
        self.center_y = 0.0;
        self.scale = 3.5 / self.width as f64;
        self.max_iter = MAX_ITER;
    }
}

// ---------------------------------------------------------------------------
// Escape time algorithm  (z_{n+1} = z_n^2 + c)
//
// Uses f64 for all arithmetic.  The bailout radius is 4.0 (|z|^2 > 4).
// Returns the iteration count at which the point escaped, or `max_iter` if
// it didn't escape (i.e., it's in the Mandelbrot set).
//
// Optimisation: we use the squared magnitude (zr*zr + zi*zi) to avoid a
// sqrt every iteration.
// ---------------------------------------------------------------------------
#[inline]
fn escape_time(c_re: f64, c_im: f64, max_iter: u32) -> u32 {
    let mut zr = 0.0_f64;
    let mut zi = 0.0_f64;
    let mut zr2 = 0.0_f64;
    let mut zi2 = 0.0_f64;

    let mut i = 0u32;
    while i < max_iter && zr2 + zi2 <= 4.0 {
        zi = 2.0 * zr * zi + c_im;
        zr = zr2 - zi2 + c_re;
        zr2 = zr * zr;
        zi2 = zi * zi;
        i += 1;
    }
    i
}

// ---------------------------------------------------------------------------
// Color mapping — converts an iteration count to an RGB triple.
//
// Uses smooth colouring via a normalised `t` value in [0, 1).  Each palette
// defines its own mapping from `t` to (R, G, B).
// ---------------------------------------------------------------------------
#[inline]
fn map_colour(iter: u32, max_iter: u32, palette: u8) -> (u8, u8, u8) {
    let t = (iter as f64) / (max_iter as f64);

    match palette {
        PALETTE_FIRE => {
            // Black → red → orange → yellow → white
            let r = (255.0 * (3.0 * t).min(1.0)) as u8;
            let g = (255.0 * (3.0 * t - 1.0).clamp(0.0, 1.0)) as u8;
            let b = (255.0 * (3.0 * t - 2.0).clamp(0.0, 1.0)) as u8;
            (r, g, b)
        }
        PALETTE_ELECTRIC => {
            // Cycle through HSL-like electric blue → magenta → cyan
            let angle = t * std::f64::consts::TAU * 3.0;
            let r = (128.0 + 127.0 * (angle).cos()) as u8;
            let g = (128.0 + 127.0 * (angle + 2.094).cos()) as u8; // +2π/3
            let b = (128.0 + 127.0 * (angle + 4.189).cos()) as u8; // +4π/3
            (r, g, b)
        }
        PALETTE_BW => {
            // Simple grayscale
            let v = (255.0 * t) as u8;
            (v, v, v)
        }
        PALETTE_OCEAN => {
            // Deep blue → cyan → white
            let r = (255.0 * (2.0 * t - 1.0).clamp(0.0, 1.0)) as u8;
            let g = (255.0 * (2.0 * t - 0.5).clamp(0.0, 1.0)) as u8;
            let b = (255.0 * (1.5 * t).min(1.0)) as u8;
            (r, g, b)
        }
        _ => {
            let v = (255.0 * t) as u8;
            (v, v, v)
        }
    }
}
