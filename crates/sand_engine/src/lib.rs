use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Cell types – stored as u8 so the grid is compact and cache-friendly.
// ---------------------------------------------------------------------------
const EMPTY: u8 = 0;
const SAND: u8 = 1;
const WATER: u8 = 2;
const STONE: u8 = 3;

// ---------------------------------------------------------------------------
// RGBA colours for each cell type (used when writing the pixel buffer).
// ---------------------------------------------------------------------------
fn cell_colour(cell: u8) -> [u8; 4] {
    match cell {
        SAND => [0xE0, 0xC0, 0x68, 0xFF],  // warm sandy yellow
        WATER => [0x40, 0x90, 0xE0, 0xFF],  // blue
        STONE => [0x78, 0x78, 0x78, 0xFF],  // grey
        _ => [0x1C, 0x1C, 0x24, 0xFF],      // background (dark)
    }
}

// ---------------------------------------------------------------------------
// Universe – the simulation state.
//
// Layout
// ------
// `cells`  : width * height  u8 values  (the automaton grid)
// `pixels` : width * height * 4  u8 values  (RGBA frame-buffer)
//
// Both buffers live inside the WASM linear memory. The JS side obtains a
// pointer + length and builds a typed-array *view* – zero-copy.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct Universe {
    width: u32,
    height: u32,
    cells: Vec<u8>,
    pixels: Vec<u8>,
}

#[wasm_bindgen]
impl Universe {
    // -- Constructor --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Universe {
        let size = (width * height) as usize;
        Universe {
            width,
            height,
            cells: vec![EMPTY; size],
            pixels: vec![0; size * 4],
        }
    }

    // -- Dimensions ---------------------------------------------------------

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    // -- Shared-memory pointers (the key to zero-copy rendering) -----------

    /// Returns a pointer to the RGBA pixel buffer inside WASM linear memory.
    /// JS constructs `new Uint8ClampedArray(wasm.memory.buffer, ptr, len)`.
    pub fn pixels_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    /// Length of the pixel buffer in bytes (width * height * 4).
    pub fn pixels_len(&self) -> usize {
        self.pixels.len()
    }

    // -- Painting -----------------------------------------------------------

    /// Paint a circle of `radius` cells centred on (cx, cy).
    pub fn paint(&mut self, cx: i32, cy: i32, cell_type: u8, radius: i32) {
        let r2 = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let x = cx + dx;
                let y = cy + dy;
                if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
                    let idx = (y as u32 * self.width + x as u32) as usize;
                    self.cells[idx] = cell_type;
                }
            }
        }
    }

    // -- Simulation ---------------------------------------------------------

    /// Advance the simulation by one step.
    ///
    /// We iterate **bottom-to-top** so that falling particles move once per
    /// tick rather than cascading through the whole column in a single frame.
    /// Within each row we randomise the scan direction to avoid left-bias
    /// for water flow.
    pub fn tick(&mut self) {
        let w = self.width as i32;
        let h = self.height as i32;
        let mut rng = rand::rng();

        // Process rows bottom-to-top (skip the very bottom row for
        // "fall-down" checks since there is nothing below it — but water
        // still needs to be processed for sideways flow).
        for y in (0..h).rev() {
            // Randomise horizontal scan direction to remove left/right bias.
            let left_to_right: bool = rng.random();
            for step in 0..w {
                let x = if left_to_right { step } else { w - 1 - step };
                let idx = (y * w + x) as usize;
                match self.cells[idx] {
                    SAND => self.tick_sand(x, y, w, h),
                    WATER => self.tick_water(x, y, w, h, &mut rng),
                    _ => {}
                }
            }
        }
    }

    // -- Render pixels ------------------------------------------------------

    /// Write the current grid state into the RGBA pixel buffer.
    /// Call this once per frame *after* `tick()`.
    pub fn render(&mut self) {
        let len = self.cells.len();
        for i in 0..len {
            let rgba = cell_colour(self.cells[i]);
            let p = i * 4;
            self.pixels[p] = rgba[0];
            self.pixels[p + 1] = rgba[1];
            self.pixels[p + 2] = rgba[2];
            self.pixels[p + 3] = rgba[3];
        }
    }

    // -- Reset --------------------------------------------------------------

    pub fn clear(&mut self) {
        self.cells.fill(EMPTY);
    }
}

// ---------------------------------------------------------------------------
// Private helpers (not exported to JS).
// ---------------------------------------------------------------------------
impl Universe {
    #[inline]
    fn idx(&self, x: i32, y: i32) -> usize {
        (y as u32 * self.width + x as u32) as usize
    }

    #[inline]
    fn get(&self, x: i32, y: i32) -> u8 {
        self.cells[self.idx(x, y)]
    }

    #[inline]
    fn swap(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let a = self.idx(x1, y1);
        let b = self.idx(x2, y2);
        self.cells.swap(a, b);
    }

    #[inline]
    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32
    }

    // -- Sand physics -------------------------------------------------------

    fn tick_sand(&mut self, x: i32, y: i32, _w: i32, h: i32) {
        if y + 1 >= h {
            return; // already at bottom
        }

        let below = self.get(x, y + 1);

        // Fall straight down into empty or water.
        if below == EMPTY {
            self.swap(x, y, x, y + 1);
            return;
        }
        if below == WATER {
            // Sand sinks through water – swap them.
            self.swap(x, y, x, y + 1);
            return;
        }

        // Try diagonal down-left / down-right (random order).
        let try_left_first: bool = rand::rng().random();
        let (dx1, dx2) = if try_left_first { (-1, 1) } else { (1, -1) };

        for dx in [dx1, dx2] {
            let nx = x + dx;
            if self.in_bounds(nx, y + 1) {
                let diag = self.get(nx, y + 1);
                if diag == EMPTY || diag == WATER {
                    self.swap(x, y, nx, y + 1);
                    return;
                }
            }
        }
    }

    // -- Water physics ------------------------------------------------------

    fn tick_water(&mut self, x: i32, y: i32, _w: i32, h: i32, rng: &mut impl Rng) {
        // Fall straight down.
        if y + 1 < h {
            let below = self.get(x, y + 1);
            if below == EMPTY {
                self.swap(x, y, x, y + 1);
                return;
            }

            // Diagonal down.
            let try_left_first: bool = rng.random();
            let (dx1, dx2) = if try_left_first { (-1, 1) } else { (1, -1) };
            for dx in [dx1, dx2] {
                let nx = x + dx;
                if self.in_bounds(nx, y + 1) && self.get(nx, y + 1) == EMPTY {
                    self.swap(x, y, nx, y + 1);
                    return;
                }
            }
        }

        // Flow sideways (water spreads horizontally).
        let try_left_first: bool = rng.random();
        let (dx1, dx2) = if try_left_first { (-1, 1) } else { (1, -1) };
        for dx in [dx1, dx2] {
            let nx = x + dx;
            if self.in_bounds(nx, y) && self.get(nx, y) == EMPTY {
                self.swap(x, y, nx, y);
                return;
            }
        }
    }
}
