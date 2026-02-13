use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const W: usize = 640;
const H: usize = 400;
const N: usize = W * H;
const GRAVITY: f32 = 0.18;
const MAX_PARTICLES: usize = 3000;
const MAX_ENTITIES: usize = 128;
const NUM_RAYS: usize = 360;
const RAY_STEP: f32 = 0.8;

// Material IDs
const EMPTY: u8 = 0;
const WOOD: u8 = 1;
const STONE: u8 = 2;
const STEEL: u8 = 3;
const GLASS: u8 = 4;
const SAND: u8 = 5;
const WATER: u8 = 6;
const FIRE: u8 = 7;
const LEAF: u8 = 8;
const DIRT: u8 = 9;
const ASH: u8 = 10;

// Entity kinds
const ENT_GRENADE: u8 = 0;
const ENT_MOLOTOV: u8 = 1;
const ENT_C4: u8 = 2;
const ENT_MISSILE: u8 = 3;

// Event kinds (must match JS)
const EVT_EXPLOSION: u8 = 0;
const EVT_BOUNCE: u8 = 1;
const EVT_FIRE_IGNITED: u8 = 2;
const EVT_MISSILE_LAUNCH: u8 = 3;
const EVT_FUSE_TICK: u8 = 4;

// Prefab kinds
const PREFAB_HOUSE: u8 = 0;
const PREFAB_TREE: u8 = 1;
const PREFAB_POND: u8 = 2;

// Particle kinds
const PK_SPARK: u8 = 0;
const PK_EMBER: u8 = 1;
const PK_SMOKE: u8 = 2;
const PK_DEBRIS: u8 = 3;

// ---------------------------------------------------------------------------
// Material properties
// ---------------------------------------------------------------------------

fn mat_color(m: u8, noise: u8) -> [u8; 4] {
    let n = noise as i16 - 20;
    let clamp = |v: i16| v.clamp(0, 255) as u8;
    match m {
        WOOD => [clamp(139 + n), clamp(90 + n / 2), clamp(43 + n / 3), 255],
        STONE => [clamp(140 + n), clamp(140 + n), clamp(140 + n), 255],
        STEEL => [clamp(180 + n / 2), clamp(195 + n / 2), clamp(210 + n / 2), 255],
        GLASS => [clamp(170 + n), clamp(215 + n), clamp(230 + n), 200],
        SAND => [clamp(210 + n), clamp(190 + n), clamp(130 + n), 255],
        WATER => [clamp(40 + n / 3), clamp(100 + n / 2), clamp(200 + n), 200],
        FIRE => [clamp(255), clamp(120 + n * 2), clamp(20 + n), 255],
        LEAF => [clamp(60 + n), clamp(140 + n), clamp(40 + n / 2), 255],
        DIRT => [clamp(100 + n), clamp(70 + n / 2), clamp(40 + n / 3), 255],
        ASH => [clamp(80 + n / 2), clamp(75 + n / 2), clamp(70 + n / 2), 200],
        _ => [0, 0, 0, 0],
    }
}

fn mat_integrity(m: u8) -> f32 {
    match m {
        WOOD => 0.25,
        STONE => 0.65,
        STEEL => 0.9,
        GLASS => 0.08,
        SAND => 0.02,
        WATER => 0.0,
        FIRE => 0.0,
        LEAF => 0.05,
        DIRT => 0.15,
        ASH => 0.01,
        _ => 0.0,
    }
}

fn is_loose(m: u8) -> bool {
    matches!(m, SAND | WATER | ASH | FIRE)
}

fn is_liquid(m: u8) -> bool {
    matches!(m, WATER)
}

fn is_solid(m: u8) -> bool {
    !matches!(m, EMPTY | WATER | FIRE)
}

// ---------------------------------------------------------------------------
// Event — flat struct for efficient WASM→JS transfer
// ---------------------------------------------------------------------------
// Layout: [kind: u8, pad: u8, pad: u8, pad: u8, x: f32, y: f32, power: f32]
// Total: 16 bytes per event, aligned

#[derive(Clone, Copy)]
#[repr(C)]
struct Event {
    kind: u8,
    _pad1: u8,
    _pad2: u8,
    _pad3: u8,
    x: f32,
    y: f32,
    power: f32,
}

impl Event {
    fn new(kind: u8, x: f32, y: f32, power: f32) -> Self {
        Event { kind, _pad1: 0, _pad2: 0, _pad3: 0, x, y, power }
    }
}

// ---------------------------------------------------------------------------
// Entity — floating-point physics object
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Entity {
    kind: u8,
    alive: bool,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    fuse: f32,       // seconds remaining (grenade = ~3s, molotov = on impact)
    bounce_count: u8,
}

impl Entity {
    fn new(kind: u8, x: f32, y: f32, vx: f32, vy: f32) -> Self {
        let fuse = match kind {
            ENT_GRENADE => 3.0,
            ENT_MOLOTOV => 99.0, // detonates on impact
            ENT_C4 => 999.0,     // manual detonation
            ENT_MISSILE => 99.0, // detonates on impact
            _ => 1.0,
        };
        Entity { kind, alive: true, x, y, vx, vy, fuse, bounce_count: 0 }
    }
}

// ---------------------------------------------------------------------------
// Visual particle
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    decay: f32,
    kind: u8,
    r: u8,
    g: u8,
    b: u8,
}

// ---------------------------------------------------------------------------
// Shockwave ring
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
// Grid cell
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    mat: u8,
    noise: u8,
    heat: f32,
    fire_ttl: u8, // fire lifetime
}

impl Cell {
    fn empty() -> Self {
        Cell { mat: EMPTY, noise: 0, heat: 0.0, fire_ttl: 0 }
    }
    fn of(mat: u8, noise: u8) -> Self {
        Cell { mat, noise, heat: 0.0, fire_ttl: if mat == FIRE { 120 } else { 0 } }
    }
}

// ---------------------------------------------------------------------------
// World — main simulation struct
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct World {
    grid: Vec<Cell>,
    scratch: Vec<Cell>,
    pixels: Vec<u8>,
    entities: Vec<Entity>,
    events: Vec<Event>,
    particles: Vec<Particle>,
    shockwaves: Vec<ShockwaveRing>,
    tick_count: u32,
    dt: f32, // fixed timestep in seconds
}

#[wasm_bindgen]
impl World {
    #[wasm_bindgen(constructor)]
    pub fn new() -> World {
        World {
            grid: vec![Cell::empty(); N],
            scratch: vec![Cell::empty(); N],
            pixels: vec![0u8; N * 4],
            entities: Vec::with_capacity(MAX_ENTITIES),
            events: Vec::with_capacity(64),
            particles: Vec::with_capacity(MAX_PARTICLES),
            shockwaves: Vec::new(),
            tick_count: 0,
            dt: 1.0 / 60.0,
        }
    }

    // -- Memory accessors for JS ------------------------------------------

    pub fn width(&self) -> u32 { W as u32 }
    pub fn height(&self) -> u32 { H as u32 }
    pub fn pixels_ptr(&self) -> *const u8 { self.pixels.as_ptr() }
    pub fn pixels_len(&self) -> usize { self.pixels.len() }

    // -- Event queue for JS -----------------------------------------------
    // Returns pointer to event array; JS reads 16 bytes per event
    pub fn events_ptr(&self) -> *const u8 {
        self.events.as_ptr() as *const u8
    }
    pub fn events_len(&self) -> usize { self.events.len() }
    pub fn events_byte_len(&self) -> usize { self.events.len() * 16 }
    pub fn clear_events(&mut self) { self.events.clear(); }

    // -- Entity accessors -------------------------------------------------
    pub fn entity_count(&self) -> u32 { self.entities.iter().filter(|e| e.alive).count() as u32 }

    // Return entity data as flat array: [kind, x, y, vx, vy, fuse] per entity
    // For trajectory rendering on JS side
    pub fn get_entity_data(&self) -> Vec<f32> {
        let mut out = Vec::new();
        for e in &self.entities {
            if !e.alive { continue; }
            out.push(e.kind as f32);
            out.push(e.x);
            out.push(e.y);
            out.push(e.vx);
            out.push(e.vy);
            out.push(e.fuse);
        }
        out
    }

    // -- Prefab stamping --------------------------------------------------

    pub fn stamp_prefab(&mut self, cx: i32, cy: i32, prefab: u8) {
        match prefab {
            PREFAB_HOUSE => self.stamp_house(cx, cy),
            PREFAB_TREE => self.stamp_tree(cx, cy),
            PREFAB_POND => self.stamp_pond(cx, cy),
            _ => {}
        }
    }

    // -- Entity spawning --------------------------------------------------

    pub fn spawn_entity(&mut self, kind: u8, x: f32, y: f32, vx: f32, vy: f32) {
        if self.entities.len() >= MAX_ENTITIES { return; }

        if kind == ENT_MISSILE {
            self.events.push(Event::new(EVT_MISSILE_LAUNCH, x, y, 1.0));
        }

        self.entities.push(Entity::new(kind, x, y, vx, vy));
    }

    // Detonate all placed C4
    pub fn detonate_c4(&mut self) {
        let c4_list: Vec<(f32, f32)> = self.entities.iter()
            .filter(|e| e.alive && e.kind == ENT_C4)
            .map(|e| (e.x, e.y))
            .collect();

        for (x, y) in &c4_list {
            self.explode(*x, *y, 160.0, 1.2);
        }

        for e in &mut self.entities {
            if e.kind == ENT_C4 { e.alive = false; }
        }
    }

    // -- Simulation tick --------------------------------------------------

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.tick_entities();
        self.tick_gravity();
        self.tick_liquids();
        self.tick_fire();
        self.tick_heat();
        self.tick_particles();
        self.tick_shockwaves();
    }

    // -- Render -----------------------------------------------------------

    pub fn render(&mut self) {
        self.render_grid();
        self.render_edge_darken();
        self.render_shockwaves();
        self.render_particles();
        self.render_entities();
    }

    // -- Reset ------------------------------------------------------------

    pub fn clear(&mut self) {
        for c in self.grid.iter_mut() { *c = Cell::empty(); }
        self.entities.clear();
        self.events.clear();
        self.particles.clear();
        self.shockwaves.clear();
        self.tick_count = 0;
    }

    pub fn cell_at(&self, x: u32, y: u32) -> u8 {
        if x >= W as u32 || y >= H as u32 { return EMPTY; }
        self.grid[y as usize * W + x as usize].mat
    }
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

impl World {
    fn idx(x: usize, y: usize) -> usize { y * W + x }

    fn in_bounds(x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H
    }


    // =====================================================================
    // Prefabs
    // =====================================================================

    fn stamp_house(&mut self, cx: i32, cy: i32) {
        let mut rng = rand::rng();
        // House: 40 wide, 30 tall
        let hw = 20i32;
        let hh = 25i32;

        // Walls (Wood)
        for y in (cy - hh)..cy {
            for x in (cx - hw)..=(cx + hw) {
                let is_wall = x == cx - hw || x == cx + hw;
                let is_floor = y == cy - 1;
                // Door opening
                let is_door = y >= cy - 10 && (x - cx).abs() <= 4;
                // Windows
                let is_window = (y >= cy - 18 && y <= cy - 14) &&
                    ((x - (cx - 10)).abs() <= 2 || (x - (cx + 10)).abs() <= 2);

                if is_door && !is_floor { continue; }
                if is_window {
                    self.set_cell(x, y, GLASS, rng.random_range(0u8..40));
                    continue;
                }
                if is_wall || is_floor {
                    self.set_cell(x, y, WOOD, rng.random_range(0u8..40));
                }
            }
        }

        // Roof (Stone) — triangle
        for row in 0..12 {
            let y = cy - hh - row;
            let half = hw + 2 - row * 2;
            for x in (cx - half)..=(cx + half) {
                self.set_cell(x, y, STONE, rng.random_range(0u8..40));
            }
        }

        // Floor/foundation (Stone)
        for x in (cx - hw - 1)..=(cx + hw + 1) {
            self.set_cell(x, cy, STONE, rng.random_range(0u8..40));
            self.set_cell(x, cy + 1, STONE, rng.random_range(0u8..40));
        }
    }

    fn stamp_tree(&mut self, cx: i32, cy: i32) {
        let mut rng = rand::rng();

        // Trunk (Wood)
        let trunk_h = 30;
        let trunk_w = 3;
        for y in (cy - trunk_h)..cy {
            for x in (cx - trunk_w / 2)..=(cx + trunk_w / 2) {
                self.set_cell(x, y, WOOD, rng.random_range(0u8..40));
            }
        }

        // Canopy (Leaf) — circular
        let canopy_r = 18i32;
        let canopy_cy = cy - trunk_h - 5;
        for dy in -canopy_r..=canopy_r {
            for dx in -canopy_r..=canopy_r {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > canopy_r * canopy_r { continue; }
                // Slight randomness at edges
                let dist = (dist_sq as f32).sqrt();
                if dist > canopy_r as f32 - 3.0 && rng.random::<f32>() < 0.4 { continue; }
                self.set_cell(cx + dx, canopy_cy + dy, LEAF, rng.random_range(0u8..40));
            }
        }
    }

    fn stamp_pond(&mut self, cx: i32, cy: i32) {
        let mut rng = rand::rng();

        // Oval pond: wide, shallow
        let rx = 25i32;
        let ry = 10i32;

        // Dirt rim
        for dy in (-ry - 3)..=(ry + 3) {
            for dx in (-rx - 3)..=(rx + 3) {
                let nx = (dx as f32) / (rx + 3) as f32;
                let ny = (dy as f32) / (ry + 3) as f32;
                if nx * nx + ny * ny > 1.0 { continue; }
                let inner_nx = (dx as f32) / rx as f32;
                let inner_ny = (dy as f32) / ry as f32;
                if inner_nx * inner_nx + inner_ny * inner_ny < 1.0 { continue; }
                self.set_cell(cx + dx, cy + dy, DIRT, rng.random_range(0u8..40));
            }
        }

        // Water fill
        for dy in -ry..=ry {
            for dx in -rx..=rx {
                let nx = (dx as f32) / rx as f32;
                let ny = (dy as f32) / ry as f32;
                if nx * nx + ny * ny > 1.0 { continue; }
                self.set_cell(cx + dx, cy + dy, WATER, rng.random_range(0u8..40));
            }
        }

        // Sand bottom
        for dx in (-rx + 2)..=(rx - 2) {
            self.set_cell(cx + dx, cy + ry + 1, SAND, rng.random_range(0u8..40));
            if rng.random::<f32>() < 0.6 {
                self.set_cell(cx + dx, cy + ry + 2, SAND, rng.random_range(0u8..40));
            }
        }
    }

    fn set_cell(&mut self, x: i32, y: i32, mat: u8, noise: u8) {
        if !Self::in_bounds(x, y) { return; }
        let idx = Self::idx(x as usize, y as usize);
        self.grid[idx] = Cell::of(mat, noise);
    }

    // =====================================================================
    // Entity physics
    // =====================================================================

    // Inline grid solid check (avoids borrow issues with entity refs)
    fn check_solid(grid: &[Cell], x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || (x as usize) >= W || (y as usize) >= H { return true; }
        is_solid(grid[y as usize * W + x as usize].mat)
    }

    fn tick_entities(&mut self) {
        let dt = self.dt;
        let mut rng = rand::rng();
        let mut deferred_explosions: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut deferred_molotov: Vec<(f32, f32)> = Vec::new();
        let mut deferred_events: Vec<Event> = Vec::new();

        for i in 0..self.entities.len() {
            if !self.entities[i].alive { continue; }

            let e = &mut self.entities[i];

            // Apply gravity (except missile which has thrust)
            match e.kind {
                ENT_MISSILE => {
                    let speed = (e.vx * e.vx + e.vy * e.vy).sqrt();
                    if speed > 0.1 {
                        let thrust = 0.35;
                        e.vx += (e.vx / speed) * thrust;
                        e.vy += (e.vy / speed) * thrust;
                    }
                    e.vy += GRAVITY * 0.15;

                    // Spawn trail particles
                    if self.particles.len() < MAX_PARTICLES {
                        self.particles.push(Particle {
                            x: e.x, y: e.y,
                            vx: rng.random_range(-0.5f32..0.5) - e.vx * 0.1,
                            vy: rng.random_range(-0.5f32..0.5) - e.vy * 0.1,
                            life: 1.0, decay: rng.random_range(0.04..0.08),
                            kind: PK_EMBER,
                            r: 255, g: rng.random_range(100..200), b: 30,
                        });
                    }
                }
                ENT_C4 => {
                    let below_x = e.x.round() as i32;
                    let below_y = (e.y + 1.0).round() as i32;
                    if !Self::check_solid(&self.grid, below_x, below_y) {
                        e.vy += GRAVITY;
                        e.vy = e.vy.min(4.0);
                    } else {
                        e.vy = 0.0;
                        e.vx = 0.0;
                    }
                }
                _ => {
                    e.vy += GRAVITY;
                }
            }

            e.vx = e.vx.clamp(-12.0, 12.0);
            e.vy = e.vy.clamp(-12.0, 12.0);

            let new_x = e.x + e.vx;
            let new_y = e.y + e.vy;
            let gx = new_x.round() as i32;
            let gy = new_y.round() as i32;

            let hit_terrain = if e.kind == ENT_C4 && e.vx.abs() < 0.01 && e.vy.abs() < 0.01 {
                false
            } else {
                Self::check_solid(&self.grid, gx, gy)
            };

            if hit_terrain {
                match e.kind {
                    ENT_GRENADE => {
                        let speed = (e.vx * e.vx + e.vy * e.vy).sqrt();
                        if speed > 1.5 && e.bounce_count < 5 {
                            let ox = e.x.round() as i32;
                            let oy = e.y.round() as i32;
                            let hit_x = Self::check_solid(&self.grid, gx, oy);
                            let hit_y = Self::check_solid(&self.grid, ox, gy);
                            if hit_x { e.vx = -e.vx * 0.5; }
                            if hit_y { e.vy = -e.vy * 0.5; }
                            if !hit_x && !hit_y { e.vx *= -0.5; e.vy *= -0.5; }
                            e.bounce_count += 1;
                            deferred_events.push(Event::new(EVT_BOUNCE, e.x, e.y, speed));
                        } else {
                            e.vx = 0.0;
                            e.vy = 0.0;
                        }
                    }
                    ENT_MOLOTOV => {
                        deferred_molotov.push((e.x, e.y));
                        e.alive = false;
                    }
                    ENT_MISSILE => {
                        deferred_explosions.push((e.x, e.y, 120.0, 1.0));
                        e.alive = false;
                    }
                    ENT_C4 => {
                        e.vx = 0.0;
                        e.vy = 0.0;
                    }
                    _ => {}
                }
            } else {
                e.x = new_x;
                e.y = new_y;
            }

            if e.x < -20.0 || e.x > (W as f32 + 20.0) || e.y > (H as f32 + 20.0) || e.y < -100.0 {
                e.alive = false;
                continue;
            }

            if e.kind == ENT_GRENADE && e.alive {
                e.fuse -= dt;

                let prev = e.fuse + dt;
                let tick_interval = 0.4;
                if (prev / tick_interval).floor() > (e.fuse / tick_interval).floor() && e.fuse > 0.0 {
                    deferred_events.push(Event::new(EVT_FUSE_TICK, e.x, e.y, e.fuse));
                }

                if e.fuse <= 0.0 {
                    deferred_explosions.push((e.x, e.y, 100.0, 0.9));
                    e.alive = false;
                }
            }
        }

        // Apply deferred events
        self.events.extend(deferred_events);

        // Process deferred explosions
        for (x, y, radius, power) in deferred_explosions {
            self.explode(x, y, radius, power);
        }

        // Process deferred molotov shatters
        for (x, y) in deferred_molotov {
            self.shatter_molotov(x, y);
        }

        // Cleanup dead entities
        self.entities.retain(|e| e.alive);
    }

    // =====================================================================
    // Explosion (raycasting)
    // =====================================================================

    fn explode(&mut self, cx: f32, cy: f32, max_range: f32, initial_energy: f32) {
        let mut rng = rand::rng();
        let mut destroyed: u32 = 0;

        self.events.push(Event::new(EVT_EXPLOSION, cx, cy, max_range * initial_energy));

        for ray in 0..NUM_RAYS {
            let angle = (ray as f32) * std::f32::consts::TAU / (NUM_RAYS as f32);
            let dx = angle.cos();
            let dy = angle.sin();
            let mut energy = initial_energy;
            let mut dist: f32 = 0.0;

            while dist < max_range && energy > 0.01 {
                dist += RAY_STEP;
                let px = cx + dx * dist;
                let py = cy + dy * dist;
                let ix = px.round() as i32;
                let iy = py.round() as i32;
                if !Self::in_bounds(ix, iy) { break; }

                let idx = Self::idx(ix as usize, iy as usize);
                let mat = self.grid[idx].mat;
                if mat == EMPTY { energy *= 0.997; continue; }

                let resistance = mat_integrity(mat);
                if energy > resistance {
                    energy -= resistance;
                    energy *= 1.0 - resistance * 0.4;
                    self.grid[idx] = Cell::empty();
                    destroyed += 1;

                    // Spawn spark
                    if self.particles.len() < MAX_PARTICLES && rng.random::<f32>() < 0.3 {
                        let speed = rng.random_range(2.0f32..6.0);
                        let spread = rng.random_range(-0.3f32..0.3);
                        self.particles.push(Particle {
                            x: ix as f32, y: iy as f32,
                            vx: dx * speed + spread,
                            vy: dy * speed + spread,
                            life: 1.0, decay: rng.random_range(0.02..0.06),
                            kind: PK_SPARK,
                            r: 255, g: rng.random_range(180..255), b: rng.random_range(50..150),
                        });
                    }

                    // Smoke
                    if self.particles.len() < MAX_PARTICLES && rng.random::<f32>() < 0.1 {
                        self.particles.push(Particle {
                            x: ix as f32, y: iy as f32,
                            vx: rng.random_range(-0.5f32..0.5),
                            vy: rng.random_range(-1.5f32..-0.3),
                            life: 1.0, decay: rng.random_range(0.004..0.012),
                            kind: PK_SMOKE,
                            r: 90, g: 85, b: 80,
                        });
                    }
                } else {
                    self.grid[idx].heat = (self.grid[idx].heat + energy * 0.4).min(0.6);
                    break;
                }
            }
        }

        // Shockwave ring
        self.shockwaves.push(ShockwaveRing {
            cx, cy,
            radius: 5.0, max_radius: max_range,
            speed: 4.0, life: 1.0,
            r: 255, g: 200, b: 100,
        });

        // Debris particles in epicenter
        let debris_count = (destroyed / 4).min(50);
        for _ in 0..debris_count {
            if self.particles.len() >= MAX_PARTICLES { break; }
            let angle = rng.random_range(0.0f32..std::f32::consts::TAU);
            let speed = rng.random_range(1.0f32..5.0);
            self.particles.push(Particle {
                x: cx, y: cy,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed - 1.0,
                life: 1.0, decay: rng.random_range(0.01..0.03),
                kind: PK_DEBRIS,
                r: 140, g: 130, b: 120,
            });
        }
    }

    // =====================================================================
    // Molotov shatter → fire pixels
    // =====================================================================

    fn shatter_molotov(&mut self, cx: f32, cy: f32) {
        let mut rng = rand::rng();

        self.events.push(Event::new(EVT_FIRE_IGNITED, cx, cy, 1.0));

        // Scatter fire pixels in a radius
        let r = 15i32;
        for _ in 0..120 {
            let dx = rng.random_range(-r..=r);
            let dy = rng.random_range(-r..0);
            let x = cx as i32 + dx;
            let y = cy as i32 + dy;
            if !Self::in_bounds(x, y) { continue; }
            let idx = Self::idx(x as usize, y as usize);
            if self.grid[idx].mat == EMPTY {
                self.grid[idx] = Cell {
                    mat: FIRE,
                    noise: rng.random_range(0u8..40),
                    heat: 1.0,
                    fire_ttl: rng.random_range(60..180),
                };
            }
        }

        // Fire embers
        for _ in 0..30 {
            if self.particles.len() >= MAX_PARTICLES { break; }
            self.particles.push(Particle {
                x: cx, y: cy,
                vx: rng.random_range(-2.0f32..2.0),
                vy: rng.random_range(-3.0f32..-0.5),
                life: 1.0, decay: rng.random_range(0.01..0.04),
                kind: PK_EMBER,
                r: 255, g: rng.random_range(80..200), b: 20,
            });
        }

        // Small shockwave
        self.shockwaves.push(ShockwaveRing {
            cx, cy,
            radius: 3.0, max_radius: 20.0,
            speed: 2.0, life: 1.0,
            r: 255, g: 100, b: 30,
        });
    }

    // =====================================================================
    // Grid physics: Gravity for loose materials
    // =====================================================================

    fn tick_gravity(&mut self) {
        let mut rng = rand::rng();

        // Bottom-up scan so falling pixels don't double-process
        for y in (0..H - 1).rev() {
            for x in 0..W {
                let idx = Self::idx(x, y);
                let mat = self.grid[idx].mat;
                if !is_loose(mat) { continue; }

                let below = Self::idx(x, y + 1);

                // Try straight down
                if self.grid[below].mat == EMPTY {
                    self.grid[below] = self.grid[idx];
                    self.grid[idx] = Cell::empty();
                    continue;
                }

                // Sand/ash settles diagonally
                if mat == SAND || mat == ASH || mat == DIRT {
                    let side: i32 = if rng.random::<bool>() { -1 } else { 1 };
                    let sx = x as i32 + side;
                    if Self::in_bounds(sx, y as i32 + 1) {
                        let si = Self::idx(sx as usize, y + 1);
                        if self.grid[si].mat == EMPTY {
                            self.grid[si] = self.grid[idx];
                            self.grid[idx] = Cell::empty();
                            continue;
                        }
                    }
                    let sx2 = x as i32 - side;
                    if Self::in_bounds(sx2, y as i32 + 1) {
                        let si2 = Self::idx(sx2 as usize, y + 1);
                        if self.grid[si2].mat == EMPTY {
                            self.grid[si2] = self.grid[idx];
                            self.grid[idx] = Cell::empty();
                        }
                    }
                }
            }
        }
    }

    // =====================================================================
    // Grid physics: Liquid flow
    // =====================================================================

    fn tick_liquids(&mut self) {
        let mut rng = rand::rng();

        for y in (0..H - 1).rev() {
            for x in 0..W {
                let idx = Self::idx(x, y);
                if !is_liquid(self.grid[idx].mat) { continue; }

                // Already handled by gravity if below is empty
                let below = Self::idx(x, y + 1);
                if self.grid[below].mat == EMPTY { continue; }

                // Spread horizontally
                let side: i32 = if rng.random::<bool>() { -1 } else { 1 };
                let sx = x as i32 + side;
                if Self::in_bounds(sx, y as i32) {
                    let si = Self::idx(sx as usize, y);
                    if self.grid[si].mat == EMPTY {
                        self.grid[si] = self.grid[idx];
                        self.grid[idx] = Cell::empty();
                        continue;
                    }
                }
                let sx2 = x as i32 - side;
                if Self::in_bounds(sx2, y as i32) {
                    let si2 = Self::idx(sx2 as usize, y);
                    if self.grid[si2].mat == EMPTY {
                        self.grid[si2] = self.grid[idx];
                        self.grid[idx] = Cell::empty();
                    }
                }
            }
        }
    }

    // =====================================================================
    // Grid physics: Fire
    // =====================================================================

    fn tick_fire(&mut self) {
        let mut rng = rand::rng();
        self.scratch.copy_from_slice(&self.grid);

        for y in 0..H {
            for x in 0..W {
                let idx = Self::idx(x, y);
                if self.grid[idx].mat != FIRE { continue; }

                // Decrement fire lifetime
                if self.scratch[idx].fire_ttl > 0 {
                    self.scratch[idx].fire_ttl -= 1;
                } else {
                    // Burn out — chance to become ash or empty
                    self.scratch[idx] = if rng.random::<f32>() < 0.3 {
                        Cell::of(ASH, rng.random_range(0u8..40))
                    } else {
                        Cell::empty()
                    };
                    continue;
                }

                // Fire rises (moves up if empty above)
                if y > 0 {
                    let above = Self::idx(x, y - 1);
                    if self.grid[above].mat == EMPTY && rng.random::<f32>() < 0.15 {
                        self.scratch[above] = Cell {
                            mat: FIRE,
                            noise: rng.random_range(0u8..40),
                            heat: 0.8,
                            fire_ttl: rng.random_range(15..40),
                        };
                    }
                }

                // Spread to adjacent flammable materials
                for &(dx, dy) in &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if !Self::in_bounds(nx, ny) { continue; }
                    let nidx = Self::idx(nx as usize, ny as usize);
                    let nmat = self.grid[nidx].mat;
                    let ignite_chance = match nmat {
                        WOOD => 0.008,
                        LEAF => 0.02,
                        _ => 0.0,
                    };
                    if ignite_chance > 0.0 && rng.random::<f32>() < ignite_chance {
                        self.scratch[nidx] = Cell {
                            mat: FIRE,
                            noise: rng.random_range(0u8..40),
                            heat: 1.0,
                            fire_ttl: rng.random_range(60..150),
                        };
                    }
                }

                // Spawn ember particles
                if self.particles.len() < MAX_PARTICLES && rng.random::<f32>() < 0.02 {
                    self.particles.push(Particle {
                        x: x as f32, y: y as f32,
                        vx: rng.random_range(-0.5f32..0.5),
                        vy: rng.random_range(-1.5f32..-0.3),
                        life: 1.0, decay: rng.random_range(0.02..0.06),
                        kind: PK_EMBER,
                        r: 255, g: rng.random_range(80..200), b: 20,
                    });
                }
            }
        }

        self.grid.copy_from_slice(&self.scratch);
    }

    // =====================================================================
    // Heat dissipation
    // =====================================================================

    fn tick_heat(&mut self) {
        for cell in self.grid.iter_mut() {
            if cell.heat > 0.0 {
                cell.heat *= 0.96;
                if cell.heat < 0.01 { cell.heat = 0.0; }
            }
        }
    }

    // =====================================================================
    // Particles
    // =====================================================================

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
                PK_SPARK => { p.vy += 0.15; p.vx *= 0.96; p.vy *= 0.96; }
                PK_EMBER => { p.vy -= 0.02; p.vx *= 0.98; p.vy *= 0.98; }
                PK_SMOKE => { p.vy -= 0.03; p.vx *= 0.95; p.vy *= 0.95; }
                PK_DEBRIS => { p.vy += 0.2; p.vx *= 0.94; p.vy *= 0.94; }
                _ => {}
            }

            if p.x < -10.0 || p.x > W as f32 + 10.0 || p.y < -10.0 || p.y > H as f32 + 10.0 {
                self.particles.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    // =====================================================================
    // Shockwaves
    // =====================================================================

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

    // =====================================================================
    // Rendering
    // =====================================================================

    fn render_grid(&mut self) {
        let tc = self.tick_count;

        for y in 0..H {
            for x in 0..W {
                let idx = Self::idx(x, y);
                let cell = &self.grid[idx];
                let pix = idx * 4;

                if cell.mat == EMPTY {
                    // Sky gradient
                    let t = y as f32 / H as f32;
                    self.pixels[pix] = (12.0 + t * 8.0) as u8;
                    self.pixels[pix + 1] = (14.0 + t * 6.0) as u8;
                    self.pixels[pix + 2] = (22.0 + t * 4.0) as u8;
                    self.pixels[pix + 3] = 255;
                    continue;
                }

                let col = mat_color(cell.mat, cell.noise);
                let mut r = col[0] as f32;
                let mut g = col[1] as f32;
                let mut b = col[2] as f32;

                // Animated fire
                if cell.mat == FIRE {
                    let flicker = ((tc as f32 * 0.3 + x as f32 * 0.5 + y as f32 * 0.3).sin() * 0.4 + 0.6).clamp(0.3, 1.0);
                    r *= flicker;
                    g *= flicker;
                    b *= flicker;
                }

                // Water shimmer
                if cell.mat == WATER {
                    let shimmer = ((tc as f32 * 0.08 + x as f32 * 0.15).sin() * 15.0) as i32;
                    r = (r as i32 + shimmer).clamp(0, 255) as f32;
                    g = (g as i32 + shimmer).clamp(0, 255) as f32;
                    b = (b as i32 + shimmer / 2).clamp(0, 255) as f32;
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
                self.pixels[pix + 3] = col[3];
            }
        }
    }

    fn render_edge_darken(&mut self) {
        for y in 1..H - 1 {
            for x in 1..W - 1 {
                let idx = Self::idx(x, y);
                let mat = self.grid[idx].mat;
                if mat == EMPTY || mat == FIRE || mat == WATER { continue; }

                let has_edge =
                    self.grid[idx - 1].mat != mat ||
                    self.grid[idx + 1].mat != mat ||
                    self.grid[idx - W].mat != mat ||
                    self.grid[idx + W].mat != mat;

                if has_edge {
                    let pix = idx * 4;
                    self.pixels[pix] = (self.pixels[pix] as u16 * 7 / 10) as u8;
                    self.pixels[pix + 1] = (self.pixels[pix + 1] as u16 * 7 / 10) as u8;
                    self.pixels[pix + 2] = (self.pixels[pix + 2] as u16 * 7 / 10) as u8;
                }
            }
        }
    }

    fn render_entities(&mut self) {
        let tc = self.tick_count;

        for e in &self.entities {
            if !e.alive { continue; }
            let px = e.x.round() as i32;
            let py = e.y.round() as i32;

            let (r, g, b, size) = match e.kind {
                ENT_GRENADE => {
                    let pulse = ((tc as f32 * 0.3).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
                    ((60.0 * pulse) as u8, (180.0 * pulse) as u8, (60.0 * pulse) as u8, 3i32)
                }
                ENT_MOLOTOV => (200, 100, 50, 3),
                ENT_C4 => {
                    let pulse = ((tc as f32 * 0.15).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
                    ((255.0 * pulse) as u8, (60.0 * pulse) as u8, (40.0 * pulse) as u8, 4i32)
                }
                ENT_MISSILE => (255, 200, 80, 3),
                _ => (255, 255, 255, 2),
            };

            for dy in -size..=size {
                for dx in -size..=size {
                    if dx * dx + dy * dy > size * size { continue; }
                    let sx = px + dx;
                    let sy = py + dy;
                    if !Self::in_bounds(sx, sy) { continue; }
                    let pix = Self::idx(sx as usize, sy as usize) * 4;
                    self.pixels[pix] = r;
                    self.pixels[pix + 1] = g;
                    self.pixels[pix + 2] = b;
                    self.pixels[pix + 3] = 255;
                }
            }

            // C4 label — crosshair
            if e.kind == ENT_C4 {
                for d in -(size + 2)..=(size + 2) {
                    for &(dx, dy) in &[(d, 0i32), (0, d)] {
                        let sx = px + dx;
                        let sy = py + dy;
                        if !Self::in_bounds(sx, sy) { continue; }
                        let pix = Self::idx(sx as usize, sy as usize) * 4;
                        self.pixels[pix] = 255;
                        self.pixels[pix + 1] = 40;
                        self.pixels[pix + 2] = 40;
                        self.pixels[pix + 3] = 255;
                    }
                }
            }
        }
    }

    fn render_shockwaves(&mut self) {
        for sw in &self.shockwaves {
            let alpha = sw.life.clamp(0.0, 1.0);
            let r = sw.radius as i32;
            let thickness = 3i32;

            for dy in -r - thickness..=r + thickness {
                for dx in -r - thickness..=r + thickness {
                    let dist = ((dx * dx + dy * dy) as f32).sqrt();
                    let diff = (dist - sw.radius).abs();
                    if diff > thickness as f32 { continue; }

                    let px = sw.cx as i32 + dx;
                    let py = sw.cy as i32 + dy;
                    if !Self::in_bounds(px, py) { continue; }

                    let edge_alpha = alpha * (1.0 - diff / thickness as f32);
                    let pix = Self::idx(px as usize, py as usize) * 4;

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

    fn render_particles(&mut self) {
        for p in &self.particles {
            let px = p.x.round() as i32;
            let py = p.y.round() as i32;
            if !Self::in_bounds(px, py) { continue; }

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
                    if !Self::in_bounds(sx, sy) { continue; }
                    let pix = Self::idx(sx as usize, sy as usize) * 4;

                    match p.kind {
                        PK_SPARK | PK_EMBER => {
                            let ar = (p.r as f32 * alpha) as u16;
                            let ag = (p.g as f32 * alpha) as u16;
                            let ab = (p.b as f32 * alpha) as u16;
                            self.pixels[pix] = (self.pixels[pix] as u16 + ar).min(255) as u8;
                            self.pixels[pix + 1] = (self.pixels[pix + 1] as u16 + ag).min(255) as u8;
                            self.pixels[pix + 2] = (self.pixels[pix + 2] as u16 + ab).min(255) as u8;
                        }
                        PK_SMOKE => {
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
}
