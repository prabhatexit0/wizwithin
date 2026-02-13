use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NUM_RAYS: usize = 480;
const RAY_STEP: f64 = 0.7;
const MAX_PARTICLES: usize = 4000;

// Material IDs
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

// Particle kinds
const PK_SPARK: u8 = 0;
const PK_EMBER: u8 = 1;
const PK_SMOKE: u8 = 2;
const PK_DEBRIS: u8 = 3;

// ---------------------------------------------------------------------------
// Material properties
// ---------------------------------------------------------------------------

struct MaterialProps {
    structural_integrity: f64,
    melting_point: f64,
    color: [u8; 4],
}

fn material_props(mat: u8) -> MaterialProps {
    match mat {
        WOOD => MaterialProps {
            structural_integrity: 0.2,
            melting_point: 0.15,
            color: [139, 90, 43, 255],
        },
        STONE => MaterialProps {
            structural_integrity: 0.6,
            melting_point: 0.8,
            color: [140, 140, 140, 255],
        },
        STEEL => MaterialProps {
            structural_integrity: 0.9,
            melting_point: 0.7,
            color: [180, 195, 210, 255],
        },
        GLASS => MaterialProps {
            structural_integrity: 0.08,
            melting_point: 0.5,
            color: [170, 215, 230, 200],
        },
        MOLTEN_STEEL => MaterialProps {
            structural_integrity: 0.0,
            melting_point: 1.0,
            color: [255, 120, 20, 255],
        },
        FALLOUT => MaterialProps {
            structural_integrity: 0.0,
            melting_point: 1.0,
            color: [80, 255, 50, 180],
        },
        ASH => MaterialProps {
            structural_integrity: 0.01,
            melting_point: 0.9,
            color: [80, 75, 70, 200],
        },
        RUST => MaterialProps {
            structural_integrity: 0.1,
            melting_point: 0.5,
            color: [160, 80, 30, 255],
        },
        _ => MaterialProps {
            structural_integrity: 0.0,
            melting_point: 0.0,
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
    vx: f32,
    vy: f32,
    heat: f32,
    fallout_ttl: u16,
    noise: u8, // per-pixel texture variation
}

impl Cell {
    fn empty() -> Self {
        Cell { material: EMPTY, vx: 0.0, vy: 0.0, heat: 0.0, fallout_ttl: 0, noise: 0 }
    }
}

// ---------------------------------------------------------------------------
// Particle (visual effect only — not in cell grid)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,  // 1.0 → 0.0
    decay: f32, // subtracted per tick
    kind: u8,
    r: u8,
    g: u8,
    b: u8,
}

// ---------------------------------------------------------------------------
// Shockwave ring (expanding circle)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ShockwaveRing {
    cx: f32,
    cy: f32,
    radius: f32,
    max_radius: f32,
    speed: f32,
    life: f32,
    r: u8,
    g: u8,
    b: u8,
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
// Main simulation
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct BlastLabSim {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
    pixels: Vec<u8>,
    bombs: Vec<PlacedBomb>,
    particles: Vec<Particle>,
    shockwaves: Vec<ShockwaveRing>,
    scratch: Vec<Cell>,
    tick_count: u32,
    // Telemetry
    peak_kinetic: f64,
    peak_temp: f64,
    pixels_destroyed: u32,
    total_energy: f64,
    // Bitmask of bomb types detonated in last detonate_all (bit0=C4, bit1=Thermite, bit2=Dirty)
    detonated_mask: u8,
}

#[wasm_bindgen]
impl BlastLabSim {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> BlastLabSim {
        let n = (width * height) as usize;
        BlastLabSim {
            width,
            height,
            cells: vec![Cell::empty(); n],
            pixels: vec![0u8; n * 4],
            bombs: Vec::new(),
            particles: Vec::with_capacity(MAX_PARTICLES),
            shockwaves: Vec::new(),
            scratch: vec![Cell::empty(); n],
            tick_count: 0,
            peak_kinetic: 0.0,
            peak_temp: 0.0,
            pixels_destroyed: 0,
            total_energy: 0.0,
            detonated_mask: 0,
        }
    }

    // -- Accessors -----------------------------------------------------------

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn pixels_ptr(&self) -> *const u8 { self.pixels.as_ptr() }
    pub fn pixels_len(&self) -> usize { self.pixels.len() }
    pub fn bomb_count(&self) -> u32 { self.bombs.len() as u32 }

    // -- Telemetry -----------------------------------------------------------

    pub fn stats_peak_kinetic(&self) -> f64 { self.peak_kinetic }
    pub fn stats_peak_temp(&self) -> f64 { self.peak_temp }
    pub fn stats_pixels_destroyed(&self) -> u32 { self.pixels_destroyed }
    pub fn stats_total_energy(&self) -> f64 { self.total_energy }
    pub fn detonated_mask(&self) -> u8 { self.detonated_mask }
    pub fn particle_count(&self) -> u32 { self.particles.len() as u32 }

    // -- Drawing -------------------------------------------------------------

    pub fn paint(&mut self, cx: i32, cy: i32, material: u8, radius: i32) {
        let mut rng = rand::rng();
        let r2 = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > r2 { continue; }
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 { continue; }
                let idx = (y as u32 * self.width + x as u32) as usize;
                if material == EMPTY {
                    self.cells[idx] = Cell::empty();
                } else {
                    self.cells[idx] = Cell {
                        material,
                        vx: 0.0, vy: 0.0, heat: 0.0, fallout_ttl: 0,
                        noise: rng.random_range(0u8..40),
                    };
                }
            }
        }
    }

    // -- Bomb placement ------------------------------------------------------

    pub fn place_bomb(&mut self, x: u32, y: u32, bomb_type: u8) {
        self.bombs.push(PlacedBomb { x, y, bomb_type });
    }

    pub fn clear_bombs(&mut self) { self.bombs.clear(); }

    // -- Detonation ----------------------------------------------------------

    pub fn detonate_all(&mut self) {
        self.peak_kinetic = 0.0;
        self.peak_temp = 0.0;
        self.pixels_destroyed = 0;
        self.total_energy = 0.0;
        self.detonated_mask = 0;

        let bombs: Vec<PlacedBomb> = self.bombs.drain(..).collect();
        for bomb in &bombs {
            self.detonated_mask |= 1 << bomb.bomb_type;
            self.detonate(bomb.x, bomb.y, bomb.bomb_type);
        }
    }

    // -- Simulation tick -----------------------------------------------------

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.tick_fallout();
        self.tick_liquids();
        self.tick_velocity();
        self.tick_heat();
        self.tick_particles();
        self.tick_shockwaves();
    }

    // -- Render to pixel buffer ----------------------------------------------

    pub fn render(&mut self) {
        let w = self.width as usize;
        let h = self.height as usize;
        let tc = self.tick_count;

        // 1. Render cells with textures
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let cell = &self.cells[idx];
                let pix = idx * 4;

                if cell.material == EMPTY {
                    self.pixels[pix] = 18;
                    self.pixels[pix + 1] = 18;
                    self.pixels[pix + 2] = 22;
                    self.pixels[pix + 3] = 255;
                    continue;
                }

                let props = material_props(cell.material);
                let n = cell.noise as i16 - 20; // -20..+20 variation

                let mut r = (props.color[0] as i16 + n).clamp(0, 255) as f32;
                let mut g = (props.color[1] as i16 + n).clamp(0, 255) as f32;
                let mut b = (props.color[2] as i16 + n).clamp(0, 255) as f32;
                let a = props.color[3];

                // Animated materials
                match cell.material {
                    FALLOUT => {
                        let flicker = ((tc as f32 * 0.15 + x as f32 * 0.3 + y as f32 * 0.2).sin() * 0.4 + 0.6).clamp(0.3, 1.0);
                        r *= flicker;
                        g *= flicker;
                        b *= flicker;
                    }
                    MOLTEN_STEEL => {
                        let pulse = ((tc as f32 * 0.2 + x as f32 * 0.1).sin() * 0.3 + 0.7).clamp(0.5, 1.0);
                        r = (r * pulse + 30.0).min(255.0);
                        g *= pulse;
                    }
                    _ => {}
                }

                // Heat glow
                if cell.heat > 0.05 {
                    let t = cell.heat.min(1.0);
                    r = r + (255.0 - r) * t;
                    g = g + (180.0 - g) * t * 0.4;
                    b = b * (1.0 - t * 0.8);
                }

                self.pixels[pix] = r.clamp(0.0, 255.0) as u8;
                self.pixels[pix + 1] = g.clamp(0.0, 255.0) as u8;
                self.pixels[pix + 2] = b.clamp(0.0, 255.0) as u8;
                self.pixels[pix + 3] = a;
            }
        }

        // 2. Edge darkening — gives materials a defined outline
        // Work on a copy to avoid reading modified pixels
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let idx = y * w + x;
                let mat = self.cells[idx].material;
                if mat == EMPTY { continue; }

                let has_edge =
                    self.cells[idx - 1].material != mat ||
                    self.cells[idx + 1].material != mat ||
                    self.cells[idx - w].material != mat ||
                    self.cells[idx + w].material != mat;

                if has_edge {
                    let pix = idx * 4;
                    self.pixels[pix] = (self.pixels[pix] as u16 * 7 / 10) as u8;
                    self.pixels[pix + 1] = (self.pixels[pix + 1] as u16 * 7 / 10) as u8;
                    self.pixels[pix + 2] = (self.pixels[pix + 2] as u16 * 7 / 10) as u8;
                }
            }
        }

        // 3. Render shockwave rings
        self.render_shockwaves();

        // 4. Render particles on top
        self.render_particles();

        // 5. Render bomb markers
        let bombs_snapshot: Vec<PlacedBomb> = self.bombs.clone();
        for bomb in &bombs_snapshot {
            self.render_bomb_marker(bomb.x, bomb.y, bomb.bomb_type);
        }
    }

    // -- Reset ---------------------------------------------------------------

    pub fn clear(&mut self) {
        for c in self.cells.iter_mut() { *c = Cell::empty(); }
        self.bombs.clear();
        self.particles.clear();
        self.shockwaves.clear();
        self.peak_kinetic = 0.0;
        self.peak_temp = 0.0;
        self.pixels_destroyed = 0;
        self.total_energy = 0.0;
        self.detonated_mask = 0;
        self.tick_count = 0;
    }

    pub fn cell_at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height { return EMPTY; }
        self.cells[(y * self.width + x) as usize].material
    }
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

impl BlastLabSim {
    fn idx(&self, x: u32, y: u32) -> usize { (y * self.width + x) as usize }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    // -- Detonation dispatcher -----------------------------------------------

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
        let max_range: f64 = 180.0;
        let initial_energy: f64 = 1.2;
        let mut rng = rand::rng();
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
                if !self.in_bounds(ix, iy) { break; }

                let idx = self.idx(ix as u32, iy as u32);
                let cell = self.cells[idx];
                if cell.material == EMPTY {
                    energy *= 0.997;
                    continue;
                }

                let props = material_props(cell.material);
                let resistance = props.structural_integrity;
                if energy > peak_force { peak_force = energy; }

                if energy > resistance {
                    energy -= resistance;
                    energy *= 1.0 - resistance * 0.5;
                    self.cells[idx] = Cell::empty();
                    destroyed += 1;

                    // Spawn spark
                    if self.particles.len() < MAX_PARTICLES && rng.random::<f64>() < 0.35 {
                        let speed = rng.random_range(2.0f32..7.0);
                        let spread = rng.random_range(-0.3f32..0.3);
                        self.particles.push(Particle {
                            x: ix as f32, y: iy as f32,
                            vx: dx as f32 * speed + spread,
                            vy: dy as f32 * speed + spread,
                            life: 1.0, decay: rng.random_range(0.02..0.06),
                            kind: PK_SPARK,
                            r: 255, g: rng.random_range(180..255), b: rng.random_range(50..150),
                        });
                    }

                    // Spawn smoke at destroyed positions
                    if self.particles.len() < MAX_PARTICLES && rng.random::<f64>() < 0.12 {
                        self.particles.push(Particle {
                            x: ix as f32, y: iy as f32,
                            vx: rng.random_range(-0.5f32..0.5),
                            vy: rng.random_range(-1.5f32..-0.3),
                            life: 1.0, decay: rng.random_range(0.004..0.012),
                            kind: PK_SMOKE,
                            r: 90, g: 85, b: 80,
                        });
                    }

                    self.apply_radial_push(ix, iy, dx as f32, dy as f32, energy as f32 * 2.5);
                } else {
                    self.cells[idx].heat = (self.cells[idx].heat + (energy * 0.4) as f32).min(0.6);
                    break;
                }
            }
        }

        // Shockwave ring
        self.shockwaves.push(ShockwaveRing {
            cx: cx as f32, cy: cy as f32,
            radius: 5.0, max_radius: max_range as f32,
            speed: 4.0, life: 1.0,
            r: 255, g: 200, b: 100,
        });

        self.total_energy += destroyed as f64 * 10.0 + peak_force * 500.0;
        self.peak_kinetic = self.peak_kinetic.max(peak_force * 1000.0);
        self.pixels_destroyed += destroyed;
    }

    fn apply_radial_push(&mut self, cx: i32, cy: i32, dx: f32, dy: f32, strength: f32) {
        for oy in -1i32..=1 {
            for ox in -1i32..=1 {
                let nx = cx + ox;
                let ny = cy + oy;
                if !self.in_bounds(nx, ny) { continue; }
                let nidx = self.idx(nx as u32, ny as u32);
                if self.cells[nidx].material != EMPTY {
                    self.cells[nidx].vx += dx * strength * 0.6;
                    self.cells[nidx].vy += dy * strength * 0.6;
                }
            }
        }
    }

    // -- Thermite: Thermal melt ----------------------------------------------

    fn detonate_thermite(&mut self, cx: u32, cy: u32) {
        let radius: i32 = 50;
        let peak_temp: f64 = 1.0;
        let mut rng = rand::rng();
        let mut destroyed: u32 = 0;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > radius * radius { continue; }
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if !self.in_bounds(x, y) { continue; }

                let dist = (dist_sq as f64).sqrt();
                let temp = peak_temp * (1.0 - dist / radius as f64).max(0.0);
                let idx = self.idx(x as u32, y as u32);
                let cell = &mut self.cells[idx];

                if cell.material == EMPTY {
                    // Spawn embers in empty space near center
                    if dist < (radius as f64 * 0.4) && self.particles.len() < MAX_PARTICLES && rng.random::<f64>() < 0.02 {
                        self.particles.push(Particle {
                            x: x as f32, y: y as f32,
                            vx: rng.random_range(-0.8f32..0.8),
                            vy: rng.random_range(-2.0f32..-0.5),
                            life: 1.0, decay: rng.random_range(0.006..0.015),
                            kind: PK_EMBER,
                            r: 255, g: rng.random_range(80..180), b: 20,
                        });
                    }
                    continue;
                }

                let props = material_props(cell.material);
                cell.heat = (cell.heat + temp as f32 * 1.2).min(1.5);

                if temp > props.melting_point {
                    match cell.material {
                        STEEL => {
                            cell.material = MOLTEN_STEEL;
                            cell.vy = 0.5;
                            cell.heat = 1.0;
                            cell.noise = rng.random_range(0u8..40);
                        }
                        WOOD => {
                            cell.material = ASH;
                            cell.noise = rng.random_range(0u8..40);
                            destroyed += 1;
                        }
                        GLASS => {
                            *cell = Cell::empty();
                            destroyed += 1;
                        }
                        _ => {
                            if temp > props.melting_point * 1.5 {
                                *cell = Cell::empty();
                                destroyed += 1;
                            }
                        }
                    }
                }
            }
        }

        // Smaller, hotter shockwave ring
        self.shockwaves.push(ShockwaveRing {
            cx: cx as f32, cy: cy as f32,
            radius: 3.0, max_radius: radius as f32 * 0.8,
            speed: 2.0, life: 1.0,
            r: 255, g: 80, b: 20,
        });

        self.total_energy += destroyed as f64 * 8.0 + peak_temp * 2000.0;
        self.peak_temp = self.peak_temp.max(peak_temp * 3500.0);
        self.pixels_destroyed += destroyed;
    }

    // -- Dirty bomb: Radiation and fallout -----------------------------------

    fn detonate_dirty(&mut self, cx: u32, cy: u32) {
        let blast_radius: i32 = 35;
        let fallout_radius: i32 = 70;
        let mut rng = rand::rng();
        let mut destroyed: u32 = 0;

        for dy in -blast_radius..=blast_radius {
            for dx in -blast_radius..=blast_radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > blast_radius * blast_radius { continue; }
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if !self.in_bounds(x, y) { continue; }

                let idx = self.idx(x as u32, y as u32);
                if self.cells[idx].material != EMPTY {
                    let dist = (dist_sq as f64).sqrt();
                    let force = 1.0 - dist / blast_radius as f64;
                    let props = material_props(self.cells[idx].material);
                    if force > props.structural_integrity * 0.6 {
                        // Spawn debris particles
                        if self.particles.len() < MAX_PARTICLES && rng.random::<f64>() < 0.2 {
                            let angle = rng.random_range(0.0f32..std::f32::consts::TAU);
                            let speed = rng.random_range(1.5f32..4.0);
                            let mc = material_props(self.cells[idx].material);
                            self.particles.push(Particle {
                                x: x as f32, y: y as f32,
                                vx: angle.cos() * speed, vy: angle.sin() * speed,
                                life: 1.0, decay: rng.random_range(0.01..0.03),
                                kind: PK_DEBRIS,
                                r: mc.color[0], g: mc.color[1], b: mc.color[2],
                            });
                        }
                        self.cells[idx] = Cell::empty();
                        destroyed += 1;
                    }
                }
            }
        }

        // Scatter fallout
        for dy in -fallout_radius..=fallout_radius {
            for dx in -fallout_radius..=fallout_radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > fallout_radius * fallout_radius { continue; }
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if !self.in_bounds(x, y) { continue; }

                let idx = self.idx(x as u32, y as u32);
                if self.cells[idx].material == EMPTY {
                    let dist = (dist_sq as f64).sqrt();
                    let prob = 0.18 * (1.0 - dist / fallout_radius as f64);
                    if rng.random::<f64>() < prob {
                        self.cells[idx] = Cell {
                            material: FALLOUT, vx: 0.0, vy: 0.0, heat: 0.0,
                            fallout_ttl: rng.random_range(200..600),
                            noise: rng.random_range(0u8..40),
                        };
                    }
                }
            }
        }

        // Green-tinted shockwave
        self.shockwaves.push(ShockwaveRing {
            cx: cx as f32, cy: cy as f32,
            radius: 4.0, max_radius: fallout_radius as f32,
            speed: 2.5, life: 1.0,
            r: 80, g: 255, b: 60,
        });

        self.total_energy += destroyed as f64 * 6.0;
        self.peak_kinetic = self.peak_kinetic.max(400.0);
        self.pixels_destroyed += destroyed;
    }

    // -- Per-tick: Fallout radiation degradation ------------------------------

    fn tick_fallout(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut rng = rand::rng();

        for y in 0..h {
            for x in 0..w {
                let idx = self.idx(x, y);
                if self.cells[idx].material != FALLOUT { continue; }

                if self.cells[idx].fallout_ttl > 0 {
                    self.cells[idx].fallout_ttl -= 1;
                } else {
                    self.cells[idx] = Cell::empty();
                    continue;
                }

                if rng.random::<f64>() > 0.03 { continue; }

                let dir = rng.random_range(0u8..4);
                let (nx, ny) = match dir {
                    0 => (x as i32, y as i32 - 1),
                    1 => (x as i32 + 1, y as i32),
                    2 => (x as i32, y as i32 + 1),
                    _ => (x as i32 - 1, y as i32),
                };
                if !self.in_bounds(nx, ny) { continue; }

                let nidx = self.idx(nx as u32, ny as u32);
                match self.cells[nidx].material {
                    STEEL => {
                        self.cells[nidx].material = RUST;
                        self.cells[nidx].noise = rng.random_range(0u8..40);
                        self.pixels_destroyed += 1;
                        self.total_energy += 3.0;
                    }
                    WOOD => {
                        self.cells[nidx].material = ASH;
                        self.cells[nidx].noise = rng.random_range(0u8..40);
                        self.pixels_destroyed += 1;
                        self.total_energy += 2.0;
                    }
                    STONE => {
                        if rng.random::<f64>() < 0.2 {
                            self.cells[nidx] = Cell::empty();
                            self.pixels_destroyed += 1;
                            self.total_energy += 4.0;
                        }
                    }
                    GLASS => {
                        self.cells[nidx] = Cell::empty();
                        self.pixels_destroyed += 1;
                        self.total_energy += 1.0;
                    }
                    RUST => {
                        if rng.random::<f64>() < 0.1 {
                            self.cells[nidx] = Cell::empty();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // -- Per-tick: Liquid flow ------------------------------------------------

    fn tick_liquids(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut rng = rand::rng();

        for y in (0..h - 1).rev() {
            for x in 0..w {
                let idx = self.idx(x, y);
                if self.cells[idx].material != MOLTEN_STEEL { continue; }
                let cell = self.cells[idx];

                let below = self.idx(x, y + 1);
                if self.cells[below].material == EMPTY {
                    self.cells[below] = cell;
                    self.cells[idx] = Cell::empty();
                    self.total_energy += 0.5;
                    continue;
                }

                let side: i32 = if rng.random::<bool>() { -1 } else { 1 };
                let sx = x as i32 + side;
                if self.in_bounds(sx, y as i32 + 1) {
                    let si = self.idx(sx as u32, y + 1);
                    if self.cells[si].material == EMPTY {
                        self.cells[si] = cell;
                        self.cells[idx] = Cell::empty();
                        continue;
                    }
                }
                let sx2 = x as i32 - side;
                if self.in_bounds(sx2, y as i32 + 1) {
                    let si2 = self.idx(sx2 as u32, y + 1);
                    if self.cells[si2].material == EMPTY {
                        self.cells[si2] = cell;
                        self.cells[idx] = Cell::empty();
                    }
                }
            }
        }
    }

    // -- Per-tick: Velocity-based cell movement ------------------------------

    fn tick_velocity(&mut self) {
        let w = self.width;
        let h = self.height;
        self.scratch.copy_from_slice(&self.cells);

        for y in 0..h {
            for x in 0..w {
                let idx = self.idx(x, y);
                let cell = self.cells[idx];
                if cell.material == EMPTY { continue; }

                let speed = (cell.vx * cell.vx + cell.vy * cell.vy).sqrt();
                if speed < 0.3 {
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

                let nx = (x as f32 + cell.vx).round() as i32;
                let ny = (y as f32 + cell.vy).round() as i32;

                if self.in_bounds(nx, ny) {
                    let nidx = self.idx(nx as u32, ny as u32);
                    if self.scratch[nidx].material == EMPTY {
                        let mut moved = cell;
                        moved.vx *= 0.82;
                        moved.vy *= 0.82;
                        moved.vy += 0.35;
                        self.scratch[nidx] = moved;
                        self.scratch[idx] = Cell::empty();
                    } else {
                        self.scratch[idx].vx = 0.0;
                        self.scratch[idx].vy = 0.0;
                    }
                } else {
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
                self.total_energy += cell.heat as f64 * 0.05;
                cell.heat *= 0.96;
                if cell.heat < 0.01 { cell.heat = 0.0; }
            }
        }
    }

    // -- Per-tick: Particles -------------------------------------------------

    fn tick_particles(&mut self) {
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.life -= p.decay;
            if p.life <= 0.0 {
                self.particles.swap_remove(i);
                continue;
            }

            p.x += p.vx;
            p.y += p.vy;

            match p.kind {
                PK_SPARK => {
                    p.vy += 0.15; // gravity
                    p.vx *= 0.96;
                    p.vy *= 0.96;
                }
                PK_EMBER => {
                    p.vy -= 0.02; // slight buoyancy
                    p.vx *= 0.98;
                    p.vy *= 0.98;
                }
                PK_SMOKE => {
                    p.vy -= 0.03; // rises
                    p.vx *= 0.95;
                    p.vy *= 0.95;
                }
                PK_DEBRIS => {
                    p.vy += 0.2; // heavy gravity
                    p.vx *= 0.94;
                    p.vy *= 0.94;
                }
                _ => {}
            }

            // Remove if off-screen
            if p.x < -10.0 || p.x > self.width as f32 + 10.0
                || p.y < -10.0 || p.y > self.height as f32 + 10.0
            {
                self.particles.swap_remove(i);
                continue;
            }

            i += 1;
        }
    }

    // -- Per-tick: Shockwave rings -------------------------------------------

    fn tick_shockwaves(&mut self) {
        let mut i = 0;
        while i < self.shockwaves.len() {
            let sw = &mut self.shockwaves[i];
            sw.radius += sw.speed;
            sw.life -= 0.025;
            if sw.life <= 0.0 || sw.radius > sw.max_radius {
                self.shockwaves.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    // -- Render: Particles ---------------------------------------------------

    fn render_particles(&mut self) {
        let w = self.width as usize;
        let h = self.height as usize;

        for p in &self.particles {
            let px = p.x.round() as i32;
            let py = p.y.round() as i32;
            if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 { continue; }

            let alpha = p.life.clamp(0.0, 1.0);
            let size = match p.kind {
                PK_SPARK => if p.life > 0.5 { 1 } else { 0 },
                PK_SMOKE => 2,
                PK_EMBER => 1,
                PK_DEBRIS => 1,
                _ => 0,
            };

            for oy in 0..=size {
                for ox in 0..=size {
                    let sx = px + ox;
                    let sy = py + oy;
                    if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 { continue; }
                    let pix = (sy as usize * w + sx as usize) * 4;

                    match p.kind {
                        PK_SPARK | PK_EMBER => {
                            // Additive blend
                            let ar = (p.r as f32 * alpha) as u16;
                            let ag = (p.g as f32 * alpha) as u16;
                            let ab = (p.b as f32 * alpha) as u16;
                            self.pixels[pix] = (self.pixels[pix] as u16 + ar).min(255) as u8;
                            self.pixels[pix + 1] = (self.pixels[pix + 1] as u16 + ag).min(255) as u8;
                            self.pixels[pix + 2] = (self.pixels[pix + 2] as u16 + ab).min(255) as u8;
                        }
                        PK_SMOKE => {
                            // Blend toward dark gray
                            let t = alpha * 0.5;
                            self.pixels[pix] = (self.pixels[pix] as f32 * (1.0 - t) + p.r as f32 * t) as u8;
                            self.pixels[pix + 1] = (self.pixels[pix + 1] as f32 * (1.0 - t) + p.g as f32 * t) as u8;
                            self.pixels[pix + 2] = (self.pixels[pix + 2] as f32 * (1.0 - t) + p.b as f32 * t) as u8;
                        }
                        PK_DEBRIS => {
                            let t = alpha;
                            self.pixels[pix] = (self.pixels[pix] as f32 * (1.0 - t) + p.r as f32 * t) as u8;
                            self.pixels[pix + 1] = (self.pixels[pix + 1] as f32 * (1.0 - t) + p.g as f32 * t) as u8;
                            self.pixels[pix + 2] = (self.pixels[pix + 2] as f32 * (1.0 - t) + p.b as f32 * t) as u8;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // -- Render: Shockwave rings ---------------------------------------------

    fn render_shockwaves(&mut self) {
        let w = self.width as i32;
        let h = self.height as i32;

        for sw in &self.shockwaves {
            let alpha = sw.life.clamp(0.0, 1.0);
            let r = sw.radius as i32;
            let thickness = 3i32;

            // Draw circle outline using midpoint check
            for dy in -r - thickness..=r + thickness {
                for dx in -r - thickness..=r + thickness {
                    let dist = ((dx * dx + dy * dy) as f32).sqrt();
                    let diff = (dist - sw.radius).abs();
                    if diff > thickness as f32 { continue; }

                    let px = sw.cx as i32 + dx;
                    let py = sw.cy as i32 + dy;
                    if px < 0 || py < 0 || px >= w || py >= h { continue; }

                    let edge_alpha = alpha * (1.0 - diff / thickness as f32);
                    let pix = (py as usize * self.width as usize + px as usize) * 4;

                    let ar = (sw.r as f32 * edge_alpha) as u16;
                    let ag = (sw.g as f32 * edge_alpha) as u16;
                    let ab = (sw.b as f32 * edge_alpha) as u16;
                    self.pixels[pix] = (self.pixels[pix] as u16 + ar).min(255) as u8;
                    self.pixels[pix + 1] = (self.pixels[pix + 1] as u16 + ag).min(255) as u8;
                    self.pixels[pix + 2] = (self.pixels[pix + 2] as u16 + ab).min(255) as u8;
                }
            }
        }
    }

    // -- Render: Bomb markers ------------------------------------------------

    fn render_bomb_marker(&mut self, cx: u32, cy: u32, bomb_type: u8) {
        let (r, g, b) = match bomb_type {
            BOMB_C4 => (255u8, 160, 40),
            BOMB_THERMITE => (255, 60, 60),
            BOMB_DIRTY => (60, 255, 80),
            _ => (255, 255, 255),
        };

        let w = self.width as i32;
        let h = self.height as i32;
        let marker_r = 6i32;
        let pulse = ((self.tick_count as f32 * 0.15).sin() * 0.3 + 0.7).clamp(0.4, 1.0);

        for dy in -marker_r..=marker_r {
            for dx in -marker_r..=marker_r {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > marker_r * marker_r { continue; }
                let px = cx as i32 + dx;
                let py = cy as i32 + dy;
                if px < 0 || py < 0 || px >= w || py >= h { continue; }
                let pix = (py as usize * self.width as usize + px as usize) * 4;

                let on_ring = dist_sq > (marker_r - 2) * (marker_r - 2);
                let on_cross = dx.abs() <= 1 || dy.abs() <= 1;
                if on_ring || on_cross {
                    self.pixels[pix] = (r as f32 * pulse) as u8;
                    self.pixels[pix + 1] = (g as f32 * pulse) as u8;
                    self.pixels[pix + 2] = (b as f32 * pulse) as u8;
                    self.pixels[pix + 3] = 255;
                }
            }
        }
    }
}
