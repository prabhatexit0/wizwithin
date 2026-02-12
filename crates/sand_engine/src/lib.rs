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
const SOIL: u8 = 9;
const SEED: u8 = 10;
const FRUIT: u8 = 11;

// ---------------------------------------------------------------------------
// Lifetime limits for ephemeral elements (in ticks).
// ---------------------------------------------------------------------------
const FIRE_MAX_LIFE: u8 = 40;
const SMOKE_MAX_LIFE: u8 = 60;
const STEAM_MAX_LIFE: u8 = 80;
const SEED_GERMINATE_TICKS: u8 = 120; // ~2 seconds at 60 fps
const PLANT_MATURITY: u8 = 200;       // ticks before a plant can fruit

// ---------------------------------------------------------------------------
// Creature constants
// ---------------------------------------------------------------------------
const CREATURE_STRIDE: usize = 5; // x, y, species, energy, state
const SPECIES_RABBIT: f32 = 0.0;
const SPECIES_FISH: f32 = 1.0;
const SPECIES_BIRD: f32 = 2.0;
const STATE_IDLE: f32 = 0.0;
const STATE_MOVING: f32 = 1.0;
const STATE_EATING: f32 = 2.0;
const MAX_ENERGY: f32 = 200.0;
const ENERGY_DRAIN: f32 = 0.15;
const ENERGY_FROM_FOOD: f32 = 60.0;

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
        SOIL => [0x5C, 0x3A, 0x1E, 0xFF],    // rich dark brown
        SEED => [0xD2, 0xB4, 0x8C, 0xFF],    // light tan
        FRUIT => [0xE8, 0x22, 0x22, 0xFF],   // bright red
        _ => [0x1C, 0x1C, 0x24, 0xFF],       // background (dark)
    }
}

/// Creature colours (drawn on top of the grid).
fn creature_colour(species: f32) -> [u8; 4] {
    if species == SPECIES_RABBIT {
        [0xF0, 0xC0, 0xD0, 0xFF] // pink/white
    } else if species == SPECIES_FISH {
        [0xFF, 0x8C, 0x00, 0xFF] // orange
    } else {
        [0x60, 0xB0, 0xF0, 0xFF] // blue/yellow
    }
}

// ---------------------------------------------------------------------------
// Helper: check if a cell type is solid ground for walking creatures.
// ---------------------------------------------------------------------------
#[inline]
fn is_solid(cell: u8) -> bool {
    matches!(cell, SAND | STONE | WOOD | SOIL | PLANT)
}

// ---------------------------------------------------------------------------
// Universe – the simulation state.
//
// Layout
// ------
// `cells`     : width * height  u8 values  (the automaton grid)
// `life`      : width * height  u8 values  (remaining lifetime / maturity)
// `pixels`    : width * height * 4  u8 values  (RGBA frame-buffer)
// `creatures` : flat f32 vec  (CREATURE_STRIDE floats per creature)
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
    creatures: Vec<f32>,
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
            creatures: Vec::new(),
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

    // -- Creature buffer pointers ------------------------------------------

    pub fn creatures_ptr(&self) -> *const f32 {
        self.creatures.as_ptr()
    }

    pub fn creatures_count(&self) -> usize {
        self.creatures.len() / CREATURE_STRIDE
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
                        SEED => 0, // germination counter starts at 0
                        _ => 0,
                    };
                }
            }
        }
    }

    // -- Creature spawning --------------------------------------------------

    /// Spawn a creature at grid coordinates (gx, gy).
    /// `species`: 0 = Rabbit, 1 = Fish, 2 = Bird.
    pub fn spawn_creature(&mut self, gx: f32, gy: f32, species: u8) {
        let sp = match species {
            0 => SPECIES_RABBIT,
            1 => SPECIES_FISH,
            2 => SPECIES_BIRD,
            _ => return,
        };
        self.creatures.push(gx);
        self.creatures.push(gy);
        self.creatures.push(sp);
        self.creatures.push(MAX_ENERGY);
        self.creatures.push(STATE_IDLE);
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
                    SOIL => self.tick_soil(x, y, w, h),
                    SEED => self.tick_seed(x, y, w, h, &mut rng),
                    FRUIT => self.tick_fruit(x, y, w, h),
                    _ => {}
                }
            }
        }

        // Update creatures after the grid tick.
        self.update_creatures();
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

        // Draw creatures on top of the grid.
        self.render_creatures();
    }

    // -- Reset --------------------------------------------------------------

    pub fn clear(&mut self) {
        self.cells.fill(EMPTY);
        self.life.fill(0);
        self.creatures.clear();
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

    // -- Soil physics (like sand, darker) -----------------------------------

    fn tick_soil(&mut self, x: i32, y: i32, _w: i32, h: i32) {
        if y + 1 >= h {
            return;
        }

        let below = self.get(x, y + 1);

        if below == EMPTY {
            self.swap(x, y, x, y + 1);
            return;
        }

        if below == WATER {
            self.swap(x, y, x, y + 1);
            return;
        }

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

    // -- Seed germination ---------------------------------------------------

    fn tick_seed(&mut self, x: i32, y: i32, _w: i32, h: i32, rng: &mut impl Rng) {
        // Seeds fall like sand first.
        if y + 1 < h {
            let below = self.get(x, y + 1);
            if below == EMPTY {
                self.swap(x, y, x, y + 1);
                return;
            }
            if below == WATER {
                self.swap(x, y, x, y + 1);
                return;
            }
        }

        // If resting on SOIL, increment germination counter.
        if y + 1 < h && self.get(x, y + 1) == SOIL {
            let i = self.idx(x, y);
            // Also require water nearby for germination.
            let has_water = self.find_neighbour(x, y, WATER).is_some();
            if has_water {
                if self.life[i] >= SEED_GERMINATE_TICKS {
                    // Germinate! Become a plant.
                    self.set(x, y, PLANT, 0);
                } else {
                    self.life[i] = self.life[i].saturating_add(1);
                }
            }
        } else {
            // Not on soil – small random chance to still try diagonals.
            let try_left_first: bool = rng.random();
            let (dx1, dx2) = if try_left_first { (-1, 1) } else { (1, -1) };
            for dx in [dx1, dx2] {
                let nx = x + dx;
                if y + 1 < h && self.in_bounds(nx, y + 1) {
                    let diag = self.get(nx, y + 1);
                    if diag == EMPTY || diag == WATER {
                        self.swap(x, y, nx, y + 1);
                        return;
                    }
                }
            }
        }
    }

    // -- Fruit physics (falls like sand) ------------------------------------

    fn tick_fruit(&mut self, x: i32, y: i32, _w: i32, h: i32) {
        if y + 1 >= h {
            return;
        }

        let below = self.get(x, y + 1);

        if below == EMPTY {
            self.swap(x, y, x, y + 1);
            return;
        }

        if below == WATER {
            self.swap(x, y, x, y + 1);
            return;
        }

        // Try diagonal.
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
                // Extinguish: fire + water -> smoke + steam
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

    // -- Plant growth & fruiting --------------------------------------------

    fn tick_plant(&mut self, x: i32, y: i32, _w: i32, _h: i32, rng: &mut impl Rng) {
        let i = self.idx(x, y);

        // Increment maturity counter.
        self.life[i] = self.life[i].saturating_add(1);

        // Fruiting: mature plants have a low chance to spawn a FRUIT adjacent.
        if self.life[i] >= PLANT_MATURITY {
            // 0.5% chance per tick to fruit.
            if rng.random_range(0u32..200) == 0 {
                // Try to place fruit in an adjacent empty cell.
                let fruit_dirs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
                for (dx, dy) in fruit_dirs {
                    let nx = x + dx;
                    let ny = y + dy;
                    if self.in_bounds(nx, ny) && self.get(nx, ny) == EMPTY {
                        self.set(nx, ny, FRUIT, 0);
                        break;
                    }
                }
            }
        }

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

    // -----------------------------------------------------------------------
    // Creature AI
    // -----------------------------------------------------------------------

    fn update_creatures(&mut self) {
        let w = self.width as i32;
        let h = self.height as i32;
        let mut rng = rand::rng();

        // We need to process each creature, potentially removing dead ones.
        let mut i = 0;
        while i < self.creatures.len() / CREATURE_STRIDE {
            let base = i * CREATURE_STRIDE;
            let species = self.creatures[base + 2];
            let energy = self.creatures[base + 3];

            // Drain energy.
            let new_energy = energy - ENERGY_DRAIN;
            if new_energy <= 0.0 {
                // Creature dies – swap-remove it.
                let current_count = self.creatures.len() / CREATURE_STRIDE;
                let last_base = (current_count - 1) * CREATURE_STRIDE;
                if base != last_base {
                    for k in 0..CREATURE_STRIDE {
                        self.creatures[base + k] = self.creatures[last_base + k];
                    }
                }
                self.creatures.truncate(self.creatures.len() - CREATURE_STRIDE);
                // Don't increment i; re-check what moved into this slot.
                continue;
            }

            self.creatures[base + 3] = new_energy;

            if species == SPECIES_RABBIT {
                self.tick_rabbit(base, w, h, &mut rng);
            } else if species == SPECIES_FISH {
                self.tick_fish(base, w, h, &mut rng);
            } else {
                self.tick_bird(base, w, h, &mut rng);
            }

            i += 1;
        }
    }

    fn tick_rabbit(&mut self, base: usize, w: i32, h: i32, rng: &mut impl Rng) {
        let mut x = self.creatures[base];
        let mut y = self.creatures[base + 1];
        let gx = x as i32;
        let gy = y as i32;

        // --- Eating: check for PLANT or FRUIT at current position or adjacent ---
        let eat_dirs: [(i32, i32); 5] = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)];
        let mut ate = false;
        for (dx, dy) in eat_dirs {
            let nx = gx + dx;
            let ny = gy + dy;
            if self.in_bounds(nx, ny) {
                let cell = self.get(nx, ny);
                if cell == PLANT || cell == FRUIT {
                    self.set(nx, ny, EMPTY, 0);
                    self.creatures[base + 3] =
                        (self.creatures[base + 3] + ENERGY_FROM_FOOD).min(MAX_ENERGY);
                    self.creatures[base + 4] = STATE_EATING;
                    ate = true;
                    break;
                }
            }
        }

        if ate {
            return;
        }

        // --- Gravity: fall if nothing solid below ---
        let below_y = gy + 1;
        if below_y < h {
            if !self.in_bounds(gx, below_y) || !is_solid(self.get(gx, below_y)) {
                y += 1.0;
                self.creatures[base + 1] = y.min((h - 1) as f32);
                self.creatures[base + 4] = STATE_MOVING;
                return;
            }
        }

        // --- Walking: move horizontally ---
        let dir: i32 = if rng.random::<bool>() { 1 } else { -1 };
        let nx = gx + dir;

        if self.in_bounds(nx, gy) {
            let ahead = self.get(nx, gy);
            if !is_solid(ahead) {
                // Path is clear (EMPTY or WATER), walk.
                x += dir as f32;
                self.creatures[base] = x.clamp(0.0, (w - 1) as f32);
                self.creatures[base + 4] = STATE_MOVING;
            } else {
                // Hit a wall – try to jump (move up 1-2 cells).
                if self.in_bounds(nx, gy - 1) && !is_solid(self.get(nx, gy - 1)) {
                    // Can jump over 1-high wall.
                    x += dir as f32;
                    y -= 1.0;
                    self.creatures[base] = x.clamp(0.0, (w - 1) as f32);
                    self.creatures[base + 1] = y.max(0.0);
                    self.creatures[base + 4] = STATE_MOVING;
                } else if self.in_bounds(nx, gy - 2)
                    && !is_solid(self.get(nx, gy - 2))
                    && self.in_bounds(gx, gy - 1)
                    && !is_solid(self.get(gx, gy - 1))
                {
                    // Jump over 2-high wall.
                    x += dir as f32;
                    y -= 2.0;
                    self.creatures[base] = x.clamp(0.0, (w - 1) as f32);
                    self.creatures[base + 1] = y.max(0.0);
                    self.creatures[base + 4] = STATE_MOVING;
                } else {
                    self.creatures[base + 4] = STATE_IDLE;
                }
            }
        } else {
            self.creatures[base + 4] = STATE_IDLE;
        }
    }

    fn tick_fish(&mut self, base: usize, _w: i32, h: i32, rng: &mut impl Rng) {
        let x = self.creatures[base];
        let y = self.creatures[base + 1];
        let gx = x as i32;
        let gy = y as i32;

        // Fish MUST be in water. If current cell isn't water, take damage.
        if !self.in_bounds(gx, gy) || self.get(gx, gy) != WATER {
            // Rapid energy drain when out of water.
            self.creatures[base + 3] -= 2.0;
            self.creatures[base + 4] = STATE_IDLE;

            // Try to fall back into water (gravity).
            if gy + 1 < h && self.in_bounds(gx, gy + 1) && self.get(gx, gy + 1) == WATER {
                self.creatures[base + 1] = (gy + 1) as f32;
            }
            return;
        }

        // --- Eating: check for PLANT or FRUIT nearby ---
        let eat_dirs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in eat_dirs {
            let nx = gx + dx;
            let ny = gy + dy;
            if self.in_bounds(nx, ny) {
                let cell = self.get(nx, ny);
                if cell == PLANT || cell == FRUIT {
                    self.set(nx, ny, EMPTY, 0);
                    self.creatures[base + 3] =
                        (self.creatures[base + 3] + ENERGY_FROM_FOOD).min(MAX_ENERGY);
                    self.creatures[base + 4] = STATE_EATING;
                    return;
                }
            }
        }

        // --- Swimming: move in a random cardinal direction, but ONLY into water ---
        let dirs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        let start = rng.random_range(0usize..4);
        for offset in 0..4 {
            let (dx, dy) = dirs[(start + offset) % 4];
            let nx = gx + dx;
            let ny = gy + dy;
            if self.in_bounds(nx, ny) && self.get(nx, ny) == WATER {
                self.creatures[base] = nx as f32;
                self.creatures[base + 1] = ny as f32;
                self.creatures[base + 4] = STATE_MOVING;
                return;
            }
        }

        // Stuck – stay idle.
        self.creatures[base + 4] = STATE_IDLE;
    }

    fn tick_bird(&mut self, base: usize, _w: i32, h: i32, rng: &mut impl Rng) {
        let x = self.creatures[base];
        let y = self.creatures[base + 1];
        let energy = self.creatures[base + 3];
        let gx = x as i32;
        let gy = y as i32;

        // --- Eating: check for PLANT or FRUIT at or adjacent to position ---
        let eat_dirs: [(i32, i32); 5] = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in eat_dirs {
            let nx = gx + dx;
            let ny = gy + dy;
            if self.in_bounds(nx, ny) {
                let cell = self.get(nx, ny);
                if cell == PLANT || cell == FRUIT {
                    self.set(nx, ny, EMPTY, 0);
                    self.creatures[base + 3] =
                        (self.creatures[base + 3] + ENERGY_FROM_FOOD).min(MAX_ENERGY);
                    self.creatures[base + 4] = STATE_EATING;
                    return;
                }
            }
        }

        // --- Hunger-driven food seeking ---
        // When energy is below half, scan a wider area for food and fly toward it.
        let hungry = energy < MAX_ENERGY * 0.5;
        if hungry {
            // Scan in a radius below and around the bird for food.
            let scan_radius: i32 = 8;
            let mut best_food: Option<(i32, i32)> = None;
            let mut best_dist = i32::MAX;
            for sy in gy - scan_radius..=gy + scan_radius {
                for sx in gx - scan_radius..=gx + scan_radius {
                    if self.in_bounds(sx, sy) {
                        let cell = self.get(sx, sy);
                        if cell == PLANT || cell == FRUIT {
                            let dist = (sx - gx).abs() + (sy - gy).abs();
                            if dist < best_dist {
                                best_dist = dist;
                                best_food = Some((sx, sy));
                            }
                        }
                    }
                }
            }
            if let Some((fx, fy)) = best_food {
                // Fly toward the food.
                let dx = (fx - gx).signum();
                let dy = (fy - gy).signum();
                let nx = gx + dx;
                let ny = gy + dy;
                if self.in_bounds(nx, ny) && self.get(nx, ny) == EMPTY {
                    self.creatures[base] = nx as f32;
                    self.creatures[base + 1] = ny as f32;
                    self.creatures[base + 4] = STATE_MOVING;
                    return;
                }
                // If direct path blocked, try just horizontal or just vertical.
                if dx != 0 && self.in_bounds(gx + dx, gy) && self.get(gx + dx, gy) == EMPTY {
                    self.creatures[base] = (gx + dx) as f32;
                    self.creatures[base + 4] = STATE_MOVING;
                    return;
                }
                if dy != 0 && self.in_bounds(gx, gy + dy) && self.get(gx, gy + dy) == EMPTY {
                    self.creatures[base + 1] = (gy + dy) as f32;
                    self.creatures[base + 4] = STATE_MOVING;
                    return;
                }
            }
        }

        // --- Flying: random wander through EMPTY space ---
        let dx: i32 = rng.random_range(-1i32..=1);
        // When hungry and no food spotted, bias downward to forage near the ground.
        // When sated, keep a mild upward bias.
        let dy: i32 = if hungry {
            if rng.random_range(0u32..3) == 0 { -1 } else { 1 } // downward bias
        } else {
            if rng.random_range(0u32..3) == 0 { 1 } else { -1 } // upward bias
        };

        // Occasionally rest: 5% chance to try to land on a surface.
        if rng.random_range(0u32..20) == 0 {
            // Try to rest by moving down onto solid ground.
            if gy + 1 < h && self.in_bounds(gx, gy + 1) && is_solid(self.get(gx, gy + 1)) {
                self.creatures[base + 4] = STATE_IDLE;
                return;
            }
        }

        let nx = gx + dx;
        let ny = gy + dy;
        if self.in_bounds(nx, ny) {
            let target = self.get(nx, ny);
            if target == EMPTY {
                self.creatures[base] = nx as f32;
                self.creatures[base + 1] = ny as f32;
                self.creatures[base + 4] = STATE_MOVING;
                return;
            }
        }

        // If blocked, try another direction.
        let fallback_dx = -dx;
        let fallback_dy = -dy;
        let fbx = gx + fallback_dx;
        let fby = gy + fallback_dy;
        if self.in_bounds(fbx, fby) && self.get(fbx, fby) == EMPTY {
            self.creatures[base] = fbx as f32;
            self.creatures[base + 1] = fby as f32;
            self.creatures[base + 4] = STATE_MOVING;
        } else {
            self.creatures[base + 4] = STATE_IDLE;
        }
    }

    // -----------------------------------------------------------------------
    // Creature rendering (drawn on top of the cell grid)
    // -----------------------------------------------------------------------

    fn render_creatures(&mut self) {
        let w = self.width as i32;
        let count = self.creatures.len() / CREATURE_STRIDE;

        for i in 0..count {
            let base = i * CREATURE_STRIDE;
            let cx = self.creatures[base] as i32;
            let cy = self.creatures[base + 1] as i32;
            let species = self.creatures[base + 2];
            let rgba = creature_colour(species);

            // Draw a 2x2 block for each creature.
            for dy in 0..2i32 {
                for dx in 0..2i32 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                        let p = ((py * w + px) as usize) * 4;
                        self.pixels[p] = rgba[0];
                        self.pixels[p + 1] = rgba[1];
                        self.pixels[p + 2] = rgba[2];
                        self.pixels[p + 3] = rgba[3];
                    }
                }
            }

            // Draw a 1-pixel eye to give the creature character.
            let eye_x = cx;
            let eye_y = cy;
            if eye_x >= 0
                && eye_x < self.width as i32
                && eye_y >= 0
                && eye_y < self.height as i32
            {
                let p = ((eye_y * w + eye_x) as usize) * 4;
                self.pixels[p] = 0x10;
                self.pixels[p + 1] = 0x10;
                self.pixels[p + 2] = 0x10;
                self.pixels[p + 3] = 0xFF;
            }
        }
    }
}
