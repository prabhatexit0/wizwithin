use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Cell types – stored as u8 so the grid is compact and cache-friendly.
// ---------------------------------------------------------------------------
const EMPTY: u8 = 0;
const SAND: u8 = 1;
const WATER: u8 = 2;
const STONE: u8 = 3;
const WOOD: u8 = 4;
const FIRE: u8 = 5;
const SMOKE: u8 = 6;
const PLANT: u8 = 7;
const STEAM: u8 = 8;

// ---------------------------------------------------------------------------
// Lifetime limits for ephemeral elements (in ticks).
// ---------------------------------------------------------------------------
const FIRE_MAX_LIFE: u8 = 40;
const SMOKE_MAX_LIFE: u8 = 60;
const STEAM_MAX_LIFE: u8 = 80;

// ---------------------------------------------------------------------------
// RGBA colours for each cell type (used when writing the pixel buffer).
// ---------------------------------------------------------------------------
fn cell_colour(cell: u8, idx: usize) -> [u8; 4] {
    match cell {
        SAND => [0xE0, 0xC0, 0x68, 0xFF],  // warm sandy yellow
        WATER => [0x40, 0x90, 0xE0, 0xFF],  // blue
        STONE => [0x78, 0x78, 0x78, 0xFF],  // grey
        WOOD => [0x8B, 0x5E, 0x3C, 0xFF],   // brown
        FIRE => {
            // Vary fire colour by position for a flickering look.
            let hash = idx.wrapping_mul(2654435761) >> 4;
            match hash % 3 {
                0 => [0xFF, 0x44, 0x00, 0xFF], // red-orange
                1 => [0xFF, 0x88, 0x00, 0xFF], // orange
                _ => [0xFF, 0xCC, 0x00, 0xFF], // yellow
            }
        }
        SMOKE => [0x60, 0x60, 0x68, 0x90],  // translucent grey
        PLANT => [0x30, 0xA0, 0x30, 0xFF],   // green
        STEAM => [0xC8, 0xD8, 0xE8, 0x80],   // translucent white-blue
        _ => [0x1C, 0x1C, 0x24, 0xFF],       // background (dark)
    }
}

// ---------------------------------------------------------------------------
// Universe – the simulation state.
//
// Layout
// ------
// `cells`  : width * height  u8 values  (the automaton grid)
// `life`   : width * height  u8 values  (remaining lifetime for ephemeral cells)
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
    life: Vec<u8>,
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
            life: vec![0; size],
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
                    // Assign initial lifetime for ephemeral elements.
                    self.life[idx] = match cell_type {
                        FIRE => FIRE_MAX_LIFE,
                        SMOKE => SMOKE_MAX_LIFE,
                        STEAM => STEAM_MAX_LIFE,
                        _ => 0,
                    };
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
                    FIRE => self.tick_fire(x, y, w, h, &mut rng),
                    SMOKE => self.tick_gas(x, y, w, h, &mut rng, SMOKE),
                    STEAM => self.tick_gas(x, y, w, h, &mut rng, STEAM),
                    PLANT => self.tick_plant(x, y, w, h, &mut rng),
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
            let rgba = cell_colour(self.cells[i], i);
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
        self.life.fill(0);
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
    fn set(&mut self, x: i32, y: i32, cell: u8, life: u8) {
        let i = self.idx(x, y);
        self.cells[i] = cell;
        self.life[i] = life;
    }

    #[inline]
    fn swap(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let a = self.idx(x1, y1);
        let b = self.idx(x2, y2);
        self.cells.swap(a, b);
        self.life.swap(a, b);
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

        // Fall straight down into empty space.
        if below == EMPTY {
            self.swap(x, y, x, y + 1);
            return;
        }

        // Density swap: sand sinks through water.
        if below == WATER {
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

    // -- Fire physics & combustion ------------------------------------------

    fn tick_fire(&mut self, x: i32, y: i32, _w: i32, _h: i32, rng: &mut impl Rng) {
        let i = self.idx(x, y);

        // Decrement lifetime; decay when expired.
        if self.life[i] == 0 {
            // Turn into smoke or vanish.
            if rng.random_range(0u8..3) == 0 {
                self.set(x, y, EMPTY, 0);
            } else {
                self.set(x, y, SMOKE, SMOKE_MAX_LIFE);
            }
            return;
        }
        self.life[i] -= 1;

        // --- Reactions with neighbours (4 cardinal + 4 diagonal) ----------
        let dirs: [(i32, i32); 8] = [
            (-1, -1), (0, -1), (1, -1),
            (-1,  0),          (1,  0),
            (-1,  1), (0,  1), (1,  1),
        ];

        for (dx, dy) in dirs {
            let nx = x + dx;
            let ny = y + dy;
            if !self.in_bounds(nx, ny) {
                continue;
            }
            let neighbour = self.get(nx, ny);

            match neighbour {
                // Extinguish: fire + water → smoke + steam
                WATER => {
                    self.set(x, y, SMOKE, SMOKE_MAX_LIFE);
                    self.set(nx, ny, STEAM, STEAM_MAX_LIFE);
                    return;
                }
                // Spread to flammable materials (10% chance per neighbour).
                WOOD | PLANT => {
                    if rng.random_range(0u32..10) == 0 {
                        self.set(nx, ny, FIRE, FIRE_MAX_LIFE);
                    }
                }
                _ => {}
            }
        }

        // Fire flickers upward: try to move up into empty space.
        if self.in_bounds(x, y - 1) && self.get(x, y - 1) == EMPTY {
            // 30% chance to drift up (gives it a flickering appearance).
            if rng.random_range(0u32..3) == 0 {
                self.swap(x, y, x, y - 1);
            }
        }
    }

    // -- Plant growth -------------------------------------------------------

    fn tick_plant(&mut self, x: i32, y: i32, _w: i32, _h: i32, rng: &mut impl Rng) {
        // Plants grow only if touching water.
        let water_neighbour = self.find_neighbour(x, y, WATER);
        if water_neighbour.is_none() {
            return;
        }

        // Small chance to grow upward into empty space (2% per tick).
        if rng.random_range(0u32..50) != 0 {
            return;
        }

        // Prefer growing up, but also allow sideways growth.
        let growth_dirs: [(i32, i32); 3] = [(0, -1), (-1, -1), (1, -1)];
        for (dx, dy) in growth_dirs {
            let nx = x + dx;
            let ny = y + dy;
            if self.in_bounds(nx, ny) && self.get(nx, ny) == EMPTY {
                // Grow: place plant in the empty spot.
                self.set(nx, ny, PLANT, 0);
                // Consume the water that fuelled the growth.
                let (wx, wy) = water_neighbour.unwrap();
                self.set(wx, wy, EMPTY, 0);
                return;
            }
        }
    }

    // -- Gas physics (Smoke & Steam) ----------------------------------------

    fn tick_gas(&mut self, x: i32, y: i32, _w: i32, _h: i32, rng: &mut impl Rng, gas_type: u8) {
        let i = self.idx(x, y);

        // Decrement lifetime; vanish when expired.
        if self.life[i] == 0 {
            self.set(x, y, EMPTY, 0);
            return;
        }
        self.life[i] -= 1;

        // Rise upward.
        if self.in_bounds(x, y - 1) && self.get(x, y - 1) == EMPTY {
            self.swap(x, y, x, y - 1);
            return;
        }

        // Disperse: randomly drift left or right.
        let dx = if rng.random::<bool>() { -1 } else { 1 };

        // Try diagonal-up first, then horizontal.
        if self.in_bounds(x + dx, y - 1) && self.get(x + dx, y - 1) == EMPTY {
            self.swap(x, y, x + dx, y - 1);
        } else if self.in_bounds(x + dx, y) && self.get(x + dx, y) == EMPTY {
            self.swap(x, y, x + dx, y);
        }

        // Ignore the gas_type identity – both smoke and steam follow the same
        // movement rules; only their colour and lifetime differ.
        let _ = gas_type;
    }

    // -- Neighbour search helper --------------------------------------------

    /// Find the first cardinal neighbour of the given type and return its coords.
    fn find_neighbour(&self, x: i32, y: i32, cell_type: u8) -> Option<(i32, i32)> {
        let dirs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in dirs {
            let nx = x + dx;
            let ny = y + dy;
            if self.in_bounds(nx, ny) && self.get(nx, ny) == cell_type {
                return Some((nx, ny));
            }
        }
        None
    }
}
