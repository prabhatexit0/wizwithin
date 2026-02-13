use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NUM_RAYS: usize = 360;
const RAY_STEP: f64 = 0.8;

// Material IDs (stored in grid cells)
const EMPTY: u8 = 0;
const WOOD: u8 = 1;
const STONE: u8 = 2;
const STEEL: u8 = 3;
const GLASS: u8 = 4;
const MOLTEN_STEEL: u8 = 5;
const FALLOUT: u8 = 6;
const ASH: u8 = 7;
const RUST: u8 = 8;

// Bomb type IDs
const BOMB_C4: u8 = 0;
const BOMB_THERMITE: u8 = 1;
const BOMB_DIRTY: u8 = 2;

// Cell states
const STATE_SOLID: u8 = 0;
const STATE_LIQUID: u8 = 1;
const STATE_GAS: u8 = 2;

// ---------------------------------------------------------------------------
// Material properties
// ---------------------------------------------------------------------------

struct MaterialProps {
    structural_integrity: f64, // 0..1 — resistance to kinetic shockwave
    melting_point: f64,        // 0..1 — resistance to thermal energy
    state: u8,
    color: [u8; 4], // RGBA
}

fn material_props(mat: u8) -> MaterialProps {
    match mat {
        WOOD => MaterialProps {
            structural_integrity: 0.2,
            melting_point: 0.15,
            state: STATE_SOLID,
            color: [139, 90, 43, 255],
        },
        STONE => MaterialProps {
            structural_integrity: 0.6,
            melting_point: 0.8,
            state: STATE_SOLID,
            color: [140, 140, 140, 255],
        },
        STEEL => MaterialProps {
            structural_integrity: 0.9,
            melting_point: 0.7,
            state: STATE_SOLID,
            color: [180, 195, 210, 255],
        },
        GLASS => MaterialProps {
            structural_integrity: 0.08,
            melting_point: 0.5,
            state: STATE_SOLID,
            color: [170, 215, 230, 200],
        },
        MOLTEN_STEEL => MaterialProps {
            structural_integrity: 0.0,
            melting_point: 1.0,
            state: STATE_LIQUID,
            color: [255, 120, 20, 255],
        },
        FALLOUT => MaterialProps {
            structural_integrity: 0.0,
            melting_point: 1.0,
            state: STATE_SOLID,
            color: [80, 255, 50, 180],
        },
        ASH => MaterialProps {
            structural_integrity: 0.01,
            melting_point: 0.9,
            state: STATE_SOLID,
            color: [80, 75, 70, 200],
        },
        RUST => MaterialProps {
            structural_integrity: 0.1,
            melting_point: 0.5,
            state: STATE_SOLID,
            color: [160, 80, 30, 255],
        },
        _ => MaterialProps {
            structural_integrity: 0.0,
            melting_point: 0.0,
            state: STATE_GAS,
            color: [0, 0, 0, 0],
        },
    }
}

// ---------------------------------------------------------------------------
// Grid cell
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    material: u8,
    // Velocity for loose / liquid pixels (pixel-space per tick)
    vx: f32,
    vy: f32,
    // Temperature (0..1, only meaningful for thermal interactions)
    heat: f32,
    // Fallout lifetime counter (ticks remaining)
    fallout_ttl: u16,
}

impl Cell {
    fn empty() -> Self {
        Cell {
            material: EMPTY,
            vx: 0.0,
            vy: 0.0,
            heat: 0.0,
            fallout_ttl: 0,
        }
    }

    fn of(material: u8) -> Self {
        Cell {
            material,
            vx: 0.0,
            vy: 0.0,
            heat: 0.0,
            fallout_ttl: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Placed bomb
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct PlacedBomb {
    x: u32,
    y: u32,
    bomb_type: u8,
}

// ---------------------------------------------------------------------------
// Blast telemetry
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct BlastStats {
    peak_kinetic_force: f64,
    peak_temperature: f64,
    pixels_destroyed: u32,
}

#[wasm_bindgen]
impl BlastStats {
    #[wasm_bindgen(getter)]
    pub fn peak_kinetic_force(&self) -> f64 {
        self.peak_kinetic_force
    }

    #[wasm_bindgen(getter)]
    pub fn peak_temperature(&self) -> f64 {
        self.peak_temperature
    }

    #[wasm_bindgen(getter)]
    pub fn pixels_destroyed(&self) -> u32 {
        self.pixels_destroyed
    }
}

// ---------------------------------------------------------------------------
// Main simulation
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct BlastLabSim {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
    pixels: Vec<u8>, // RGBA output
    bombs: Vec<PlacedBomb>,
    // Cumulative telemetry (reset on detonate_all, accumulated across bombs)
    stats: BlastStats,
    // Per-tick scratch buffer for cell movement
    scratch: Vec<Cell>,
}

#[wasm_bindgen]
impl BlastLabSim {
    // -- Construction --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> BlastLabSim {
        let n = (width * height) as usize;
        BlastLabSim {
            width,
            height,
            cells: vec![Cell::empty(); n],
            pixels: vec![0u8; n * 4],
            bombs: Vec::new(),
            stats: BlastStats {
                peak_kinetic_force: 0.0,
                peak_temperature: 0.0,
                pixels_destroyed: 0,
            },
            scratch: vec![Cell::empty(); n],
        }
    }

    // -- Accessors -----------------------------------------------------------

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    pub fn pixels_len(&self) -> usize {
        self.pixels.len()
    }

    // -- Telemetry -----------------------------------------------------------

    pub fn stats_peak_kinetic(&self) -> f64 {
        self.stats.peak_kinetic_force
    }

    pub fn stats_peak_temp(&self) -> f64 {
        self.stats.peak_temperature
    }

    pub fn stats_pixels_destroyed(&self) -> u32 {
        self.stats.pixels_destroyed
    }

    pub fn bomb_count(&self) -> u32 {
        self.bombs.len() as u32
    }

    // -- Drawing -------------------------------------------------------------

    pub fn paint(&mut self, cx: i32, cy: i32, material: u8, radius: i32) {
        let r2 = (radius * radius) as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let idx = (y as u32 * self.width + x as u32) as usize;
                if material == EMPTY {
                    self.cells[idx] = Cell::empty();
                } else {
                    self.cells[idx] = Cell::of(material);
                }
            }
        }
    }

    // -- Bomb placement ------------------------------------------------------

    pub fn place_bomb(&mut self, x: u32, y: u32, bomb_type: u8) {
        self.bombs.push(PlacedBomb { x, y, bomb_type });
    }

    pub fn clear_bombs(&mut self) {
        self.bombs.clear();
    }

    // -- Detonation ----------------------------------------------------------

    pub fn detonate_all(&mut self) {
        // Reset stats for this detonation sequence
        self.stats.peak_kinetic_force = 0.0;
        self.stats.peak_temperature = 0.0;
        self.stats.pixels_destroyed = 0;

        let bombs: Vec<PlacedBomb> = self.bombs.drain(..).collect();
        for bomb in &bombs {
            self.detonate(bomb.x, bomb.y, bomb.bomb_type);
        }
    }

    // -- Simulation tick (post-detonation physics) ---------------------------

    pub fn tick(&mut self) {
        self.tick_fallout();
        self.tick_liquids();
        self.tick_velocity();
        self.tick_heat();
    }

    // -- Render to pixel buffer ----------------------------------------------

    pub fn render(&mut self) {
        let w = self.width as usize;
        let h = self.height as usize;

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let cell = &self.cells[idx];
                let pix = idx * 4;

                if cell.material == EMPTY {
                    // Dark background
                    self.pixels[pix] = 20;
                    self.pixels[pix + 1] = 20;
                    self.pixels[pix + 2] = 25;
                    self.pixels[pix + 3] = 255;
                } else {
                    let props = material_props(cell.material);
                    let mut r = props.color[0] as f32;
                    let mut g = props.color[1] as f32;
                    let mut b = props.color[2] as f32;
                    let a = props.color[3];

                    // Heat glow overlay
                    if cell.heat > 0.1 {
                        let t = cell.heat.min(1.0);
                        r = r + (255.0 - r) * t;
                        g = g + (160.0 - g) * t * 0.5;
                        b = b * (1.0 - t * 0.7);
                    }

                    self.pixels[pix] = r.clamp(0.0, 255.0) as u8;
                    self.pixels[pix + 1] = g.clamp(0.0, 255.0) as u8;
                    self.pixels[pix + 2] = b.clamp(0.0, 255.0) as u8;
                    self.pixels[pix + 3] = a;
                }
            }
        }
    }

    // -- Reset ---------------------------------------------------------------

    pub fn clear(&mut self) {
        for c in self.cells.iter_mut() {
            *c = Cell::empty();
        }
        self.bombs.clear();
        self.stats.peak_kinetic_force = 0.0;
        self.stats.peak_temperature = 0.0;
        self.stats.pixels_destroyed = 0;
    }

    pub fn cell_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return EMPTY;
        }
        self.cells[(y * self.width + x) as usize].material
    }
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

impl BlastLabSim {
    fn idx(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    // -- Core detonation dispatcher ------------------------------------------

    fn detonate(&mut self, cx: u32, cy: u32, bomb_type: u8) {
        match bomb_type {
            BOMB_C4 => self.detonate_c4(cx, cy),
            BOMB_THERMITE => self.detonate_thermite(cx, cy),
            BOMB_DIRTY => self.detonate_dirty(cx, cy),
            _ => {}
        }
    }

    // -- C4: Kinetic raycasting explosion ------------------------------------

    fn detonate_c4(&mut self, cx: u32, cy: u32) {
        let max_range: f64 = 120.0;
        let initial_energy: f64 = 1.0;

        let mut peak_force: f64 = 0.0;
        let mut destroyed: u32 = 0;

        for ray in 0..NUM_RAYS {
            let angle = (ray as f64) * std::f64::consts::TAU / (NUM_RAYS as f64);
            let dx = angle.cos();
            let dy = angle.sin();

            let mut energy = initial_energy;
            let mut dist: f64 = 0.0;

            while dist < max_range && energy > 0.01 {
                dist += RAY_STEP;
                let px = cx as f64 + dx * dist;
                let py = cy as f64 + dy * dist;
                let ix = px.round() as i32;
                let iy = py.round() as i32;

                if !self.in_bounds(ix, iy) {
                    break;
                }

                let idx = self.idx(ix as u32, iy as u32);
                let cell = self.cells[idx];

                if cell.material == EMPTY {
                    // Energy attenuates slightly through air
                    energy *= 0.998;
                    continue;
                }

                let props = material_props(cell.material);
                let resistance = props.structural_integrity;

                // The force at this point
                let force_here = energy;
                if force_here > peak_force {
                    peak_force = force_here;
                }

                if energy > resistance {
                    // Destroy the cell
                    energy -= resistance;
                    // Energy is also absorbed proportionally
                    energy *= 1.0 - resistance * 0.5;

                    self.cells[idx] = Cell::empty();
                    destroyed += 1;

                    // Push surviving neighbors outward (radial impulse)
                    self.apply_radial_push(ix, iy, dx as f32, dy as f32, energy as f32 * 2.0);
                } else {
                    // Material absorbs the ray — shockwave stops here
                    // Partially damage the cell (reduce integrity visually via heat)
                    self.cells[idx].heat = (self.cells[idx].heat + (energy * 0.3) as f32).min(0.5);
                    break;
                }
            }
        }

        self.stats.peak_kinetic_force =
            self.stats.peak_kinetic_force.max(peak_force * 1000.0);
        self.stats.pixels_destroyed += destroyed;
    }

    fn apply_radial_push(&mut self, cx: i32, cy: i32, dx: f32, dy: f32, strength: f32) {
        // Push a small area of neighbors in the ray direction
        for oy in -1i32..=1 {
            for ox in -1i32..=1 {
                let nx = cx + ox;
                let ny = cy + oy;
                if !self.in_bounds(nx, ny) {
                    continue;
                }
                let nidx = self.idx(nx as u32, ny as u32);
                if self.cells[nidx].material != EMPTY {
                    self.cells[nidx].vx += dx * strength * 0.5;
                    self.cells[nidx].vy += dy * strength * 0.5;
                }
            }
        }
    }

    // -- Thermite: Thermal melt ----------------------------------------------

    fn detonate_thermite(&mut self, cx: u32, cy: u32) {
        let radius: i32 = 30;
        let peak_temp: f64 = 1.0;
        let mut destroyed: u32 = 0;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > radius * radius {
                    continue;
                }

                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if !self.in_bounds(x, y) {
                    continue;
                }

                let dist = (dist_sq as f64).sqrt();
                // Temperature falls off with distance
                let temp = peak_temp * (1.0 - dist / radius as f64).max(0.0);

                let idx = self.idx(x as u32, y as u32);
                let cell = &mut self.cells[idx];

                if cell.material == EMPTY {
                    continue;
                }

                let props = material_props(cell.material);

                // Apply heat
                cell.heat = (cell.heat + temp as f32).min(1.5);

                if temp > props.melting_point {
                    match cell.material {
                        STEEL => {
                            // Steel melts to liquid molten steel
                            cell.material = MOLTEN_STEEL;
                            cell.vy = 0.5; // Slight downward flow
                            cell.heat = 1.0;
                        }
                        WOOD => {
                            // Wood burns away to ash
                            cell.material = ASH;
                            destroyed += 1;
                        }
                        GLASS => {
                            // Glass melts away
                            cell.material = EMPTY;
                            cell.heat = 0.0;
                            destroyed += 1;
                        }
                        _ => {
                            // Other materials just get destroyed at sufficient temp
                            if temp > props.melting_point * 1.5 {
                                *cell = Cell::empty();
                                destroyed += 1;
                            }
                        }
                    }
                }
            }
        }

        self.stats.peak_temperature = self.stats.peak_temperature.max(peak_temp * 3500.0);
        self.stats.pixels_destroyed += destroyed;
    }

    // -- Dirty bomb: Radiation and fallout -----------------------------------

    fn detonate_dirty(&mut self, cx: u32, cy: u32) {
        let blast_radius: i32 = 20;
        let fallout_radius: i32 = 50;
        let mut destroyed: u32 = 0;

        // Small kinetic core blast
        for dy in -blast_radius..=blast_radius {
            for dx in -blast_radius..=blast_radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > blast_radius * blast_radius {
                    continue;
                }

                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if !self.in_bounds(x, y) {
                    continue;
                }

                let idx = self.idx(x as u32, y as u32);
                if self.cells[idx].material != EMPTY {
                    let dist = (dist_sq as f64).sqrt();
                    let force = 1.0 - dist / blast_radius as f64;
                    let props = material_props(self.cells[idx].material);
                    if force > props.structural_integrity * 0.7 {
                        self.cells[idx] = Cell::empty();
                        destroyed += 1;
                    }
                }
            }
        }

        // Scatter fallout particles in a wider radius
        let mut rng = rand::rng();
        for dy in -fallout_radius..=fallout_radius {
            for dx in -fallout_radius..=fallout_radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > fallout_radius * fallout_radius {
                    continue;
                }

                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if !self.in_bounds(x, y) {
                    continue;
                }

                let idx = self.idx(x as u32, y as u32);
                if self.cells[idx].material == EMPTY {
                    // Probability decreases with distance
                    let dist = (dist_sq as f64).sqrt();
                    let prob = 0.15 * (1.0 - dist / fallout_radius as f64);
                    if rng.random::<f64>() < prob {
                        self.cells[idx] = Cell {
                            material: FALLOUT,
                            vx: 0.0,
                            vy: 0.0,
                            heat: 0.0,
                            fallout_ttl: rng.random_range(200..600),
                        };
                    }
                }
            }
        }

        self.stats.peak_kinetic_force = self.stats.peak_kinetic_force.max(300.0);
        self.stats.pixels_destroyed += destroyed;
    }

    // -- Per-tick: Fallout radiation degradation ------------------------------

    fn tick_fallout(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut rng = rand::rng();

        for y in 0..h {
            for x in 0..w {
                let idx = self.idx(x, y);
                if self.cells[idx].material != FALLOUT {
                    continue;
                }

                // Decrement TTL
                if self.cells[idx].fallout_ttl > 0 {
                    self.cells[idx].fallout_ttl -= 1;
                } else {
                    // Fallout decays
                    self.cells[idx] = Cell::empty();
                    continue;
                }

                // Randomly affect one neighbor
                if rng.random::<f64>() > 0.03 {
                    continue;
                }

                let dir = rng.random_range(0u8..4);
                let (nx, ny) = match dir {
                    0 => (x as i32, y as i32 - 1),
                    1 => (x as i32 + 1, y as i32),
                    2 => (x as i32, y as i32 + 1),
                    _ => (x as i32 - 1, y as i32),
                };

                if !self.in_bounds(nx, ny) {
                    continue;
                }

                let nidx = self.idx(nx as u32, ny as u32);
                match self.cells[nidx].material {
                    STEEL => {
                        // Steel degrades to rust
                        self.cells[nidx].material = RUST;
                        self.stats.pixels_destroyed += 1;
                    }
                    WOOD => {
                        // Wood degrades to ash
                        self.cells[nidx].material = ASH;
                        self.stats.pixels_destroyed += 1;
                    }
                    STONE => {
                        // Stone slowly crumbles
                        if rng.random::<f64>() < 0.2 {
                            self.cells[nidx] = Cell::empty();
                            self.stats.pixels_destroyed += 1;
                        }
                    }
                    GLASS => {
                        // Glass disintegrates
                        self.cells[nidx] = Cell::empty();
                        self.stats.pixels_destroyed += 1;
                    }
                    RUST => {
                        // Rust crumbles further
                        if rng.random::<f64>() < 0.1 {
                            self.cells[nidx] = Cell::empty();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // -- Per-tick: Liquid flow (molten steel flows downward) ------------------

    fn tick_liquids(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut rng = rand::rng();

        // Process bottom-up so liquids can flow down
        for y in (0..h - 1).rev() {
            for x in 0..w {
                let idx = self.idx(x, y);
                let cell = self.cells[idx];

                if cell.material != MOLTEN_STEEL {
                    continue;
                }

                // Try to flow straight down
                let below = self.idx(x, y + 1);
                if self.cells[below].material == EMPTY {
                    self.cells[below] = cell;
                    self.cells[idx] = Cell::empty();
                    continue;
                }

                // Try down-left or down-right
                let side: i32 = if rng.random::<bool>() { -1 } else { 1 };
                let sx = x as i32 + side;
                if self.in_bounds(sx, y as i32 + 1) {
                    let side_below = self.idx(sx as u32, y + 1);
                    if self.cells[side_below].material == EMPTY {
                        self.cells[side_below] = cell;
                        self.cells[idx] = Cell::empty();
                        continue;
                    }
                }

                // Try the other side
                let sx2 = x as i32 - side;
                if self.in_bounds(sx2, y as i32 + 1) {
                    let side_below2 = self.idx(sx2 as u32, y + 1);
                    if self.cells[side_below2].material == EMPTY {
                        self.cells[side_below2] = cell;
                        self.cells[idx] = Cell::empty();
                    }
                }
            }
        }
    }

    // -- Per-tick: Move cells with velocity (blast push) ---------------------

    fn tick_velocity(&mut self) {
        let w = self.width;
        let h = self.height;

        // Copy current state to scratch
        self.scratch.copy_from_slice(&self.cells);

        for y in 0..h {
            for x in 0..w {
                let idx = self.idx(x, y);
                let cell = self.cells[idx];

                if cell.material == EMPTY {
                    continue;
                }

                let speed = (cell.vx * cell.vx + cell.vy * cell.vy).sqrt();
                if speed < 0.3 {
                    // Apply gravity to loose particles
                    if cell.material == ASH || cell.material == RUST {
                        let gy = y + 1;
                        if gy < h {
                            let below = self.idx(x, gy);
                            if self.scratch[below].material == EMPTY {
                                self.scratch[below] = Cell { vy: 0.5, ..cell };
                                self.scratch[idx] = Cell::empty();
                            }
                        }
                    }
                    continue;
                }

                // Move the pixel toward its velocity direction
                let nx = (x as f32 + cell.vx).round() as i32;
                let ny = (y as f32 + cell.vy).round() as i32;

                if self.in_bounds(nx, ny) {
                    let nidx = self.idx(nx as u32, ny as u32);
                    if self.scratch[nidx].material == EMPTY {
                        let mut moved = cell;
                        // Dampen velocity
                        moved.vx *= 0.85;
                        moved.vy *= 0.85;
                        // Gravity
                        moved.vy += 0.3;
                        self.scratch[nidx] = moved;
                        self.scratch[idx] = Cell::empty();
                    } else {
                        // Collision — stop
                        self.scratch[idx].vx = 0.0;
                        self.scratch[idx].vy = 0.0;
                    }
                } else {
                    // Flew off-screen
                    self.scratch[idx] = Cell::empty();
                }
            }
        }

        self.cells.copy_from_slice(&self.scratch);
    }

    // -- Per-tick: Heat dissipation ------------------------------------------

    fn tick_heat(&mut self) {
        for cell in self.cells.iter_mut() {
            if cell.heat > 0.0 {
                cell.heat *= 0.97;
                if cell.heat < 0.01 {
                    cell.heat = 0.0;
                }
            }
        }
    }
}
