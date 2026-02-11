use rand::Rng;
use wasm_bindgen::prelude::*;

const DT: f32 = 1.0 / 60.0;

// ---------------------------------------------------------------------------
// Boid – a single agent (prey or predator).
//
// `#[repr(C)]` guarantees [x, y, vx, vy, eat_timer] as five contiguous f32s.
// JS reads them directly via Float32Array – zero copy.
// ---------------------------------------------------------------------------
#[repr(C)]
struct Boid {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    eat_timer: f32,
}

// ---------------------------------------------------------------------------
// FoodItem – a static morsel on the field.
//
// `#[repr(C)]` guarantees [x, y] as two contiguous f32s.
// ---------------------------------------------------------------------------
#[repr(C)]
struct FoodItem {
    x: f32,
    y: f32,
}

// ---------------------------------------------------------------------------
// Flock – the full ecosystem state exposed to JavaScript via wasm-bindgen.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct Flock {
    boids: Vec<Boid>,
    predators: Vec<Boid>,
    food: Vec<FoodItem>,

    width: f32,
    height: f32,

    // Boid flocking rule weights (tuneable from the UI).
    sep_weight: f32,
    ali_weight: f32,
    coh_weight: f32,

    // Boid physics limits.
    max_speed: f32,
    max_force: f32,
    perception: f32,

    // Predator physics.
    predator_speed: f32,
    predator_force: f32,
    predator_perception: f32,

    // Prey–predator interaction.
    fear_radius: f32,
    fear_weight: f32,
    hunger_weight: f32,
    eat_radius: f32,
}

#[wasm_bindgen]
impl Flock {
    // -- Constructor --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(width: f32, height: f32, count: u32) -> Flock {
        let boids = Self::spawn_boids(width, height, count);
        let food = Self::scatter_food(width, height, 30);

        Flock {
            boids,
            predators: Vec::new(),
            food,
            width,
            height,

            sep_weight: 1.5,
            ali_weight: 1.0,
            coh_weight: 1.0,
            max_speed: 3.0,
            max_force: 0.15,
            perception: 50.0,

            predator_speed: 3.8,
            predator_force: 0.2,
            predator_perception: 150.0,

            fear_radius: 80.0,
            fear_weight: 3.0,
            hunger_weight: 0.4,
            eat_radius: 8.0,
        }
    }

    // -- Shared-memory pointers (zero-copy rendering) -----------------------

    /// Boid buffer.  JS stride = 5 floats: [x, y, vx, vy, eat_timer].
    pub fn boids_ptr(&self) -> *const f32 {
        self.boids.as_ptr() as *const f32
    }
    pub fn boids_count(&self) -> u32 {
        self.boids.len() as u32
    }

    /// Predator buffer.  Same layout as boids (stride 5).
    pub fn predators_ptr(&self) -> *const f32 {
        self.predators.as_ptr() as *const f32
    }
    pub fn predators_count(&self) -> u32 {
        self.predators.len() as u32
    }

    /// Food buffer.  JS stride = 2 floats: [x, y].
    pub fn food_ptr(&self) -> *const f32 {
        self.food.as_ptr() as *const f32
    }
    pub fn food_count(&self) -> u32 {
        self.food.len() as u32
    }

    // -- Tuneable setters ---------------------------------------------------

    pub fn set_separation(&mut self, w: f32) {
        self.sep_weight = w;
    }
    pub fn set_alignment(&mut self, w: f32) {
        self.ali_weight = w;
    }
    pub fn set_cohesion(&mut self, w: f32) {
        self.coh_weight = w;
    }

    /// Dynamically add or remove boids to reach `count`.
    pub fn set_count(&mut self, count: u32) {
        let current = self.boids.len();
        let target = count as usize;

        if target > current {
            let mut rng = rand::rng();
            for _ in 0..(target - current) {
                let angle = rng.random::<f32>() * std::f32::consts::TAU;
                let speed = rng.random::<f32>() * 2.0 + 1.0;
                self.boids.push(Boid {
                    x: rng.random::<f32>() * self.width,
                    y: rng.random::<f32>() * self.height,
                    vx: angle.cos() * speed,
                    vy: angle.sin() * speed,
                    eat_timer: 0.0,
                });
            }
        } else if target < current {
            self.boids.truncate(target);
        }
    }

    // -- Ecosystem actions --------------------------------------------------

    /// Drop food at (x, y).  Called from JS on left-click.
    pub fn spawn_food(&mut self, x: f32, y: f32) {
        self.food.push(FoodItem { x, y });
    }

    pub fn clear_food(&mut self) {
        self.food.clear();
    }

    /// Add a predator at a random screen edge.
    pub fn spawn_predator(&mut self) {
        let mut rng = rand::rng();
        let edge = rng.random::<u32>() % 4;
        let (x, y) = match edge {
            0 => (rng.random::<f32>() * self.width, 0.0),
            1 => (rng.random::<f32>() * self.width, self.height),
            2 => (0.0, rng.random::<f32>() * self.height),
            _ => (self.width, rng.random::<f32>() * self.height),
        };
        let angle = rng.random::<f32>() * std::f32::consts::TAU;
        self.predators.push(Boid {
            x,
            y,
            vx: angle.cos() * self.predator_speed * 0.5,
            vy: angle.sin() * self.predator_speed * 0.5,
            eat_timer: 0.0,
        });
    }

    // -- Simulation step ----------------------------------------------------

    /// Advance the entire ecosystem by one tick.
    ///
    /// 1. Compute boid steering (flocking + fear + hunger).
    /// 2. Integrate boid positions, check eating, decay eat timers.
    /// 3. Compute & integrate predator positions.
    pub fn tick(&mut self) {
        self.tick_boids();
        self.tick_predators();
    }
}

// ---------------------------------------------------------------------------
// Private helpers (not exported to JS).
// ---------------------------------------------------------------------------
impl Flock {
    // -- Initialisation helpers ---------------------------------------------

    fn spawn_boids(width: f32, height: f32, count: u32) -> Vec<Boid> {
        let mut rng = rand::rng();
        let mut boids = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let angle = rng.random::<f32>() * std::f32::consts::TAU;
            let speed = rng.random::<f32>() * 2.0 + 1.0;
            boids.push(Boid {
                x: rng.random::<f32>() * width,
                y: rng.random::<f32>() * height,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                eat_timer: 0.0,
            });
        }
        boids
    }

    fn scatter_food(width: f32, height: f32, count: u32) -> Vec<FoodItem> {
        let mut rng = rand::rng();
        (0..count)
            .map(|_| FoodItem {
                x: rng.random::<f32>() * width,
                y: rng.random::<f32>() * height,
            })
            .collect()
    }

    // -- Boid tick ----------------------------------------------------------

    fn tick_boids(&mut self) {
        let n = self.boids.len();
        if n == 0 {
            return;
        }

        // First pass – compute steering acceleration for every boid.
        let mut accels: Vec<(f32, f32)> = Vec::with_capacity(n);

        for i in 0..n {
            let boid = &self.boids[i];
            let (sx, sy, ax, ay, cx, cy) = self.boid_flock_forces(i);

            // Fear: massive repulsion from nearby predators.
            let (fear_x, fear_y, feared) = self.compute_fear(boid);

            if feared {
                // Override alignment/cohesion – keep separation + fear.
                accels.push((
                    sx * self.sep_weight + fear_x * self.fear_weight,
                    sy * self.sep_weight + fear_y * self.fear_weight,
                ));
            } else {
                // Normal flocking + hunger.
                let (hx, hy) = self.compute_hunger(boid);
                accels.push((
                    sx * self.sep_weight
                        + ax * self.ali_weight
                        + cx * self.coh_weight
                        + hx * self.hunger_weight,
                    sy * self.sep_weight
                        + ay * self.ali_weight
                        + cy * self.coh_weight
                        + hy * self.hunger_weight,
                ));
            }
        }

        // Second pass – integrate velocity & position, check eating, decay timer.
        let mut eaten_indices: Vec<usize> = Vec::new();

        for i in 0..n {
            let b = &mut self.boids[i];

            // Apply acceleration.
            b.vx += accels[i].0;
            b.vy += accels[i].1;

            // Clamp speed.
            let speed = (b.vx * b.vx + b.vy * b.vy).sqrt();
            if speed > self.max_speed {
                let s = self.max_speed / speed;
                b.vx *= s;
                b.vy *= s;
            }

            // Move.
            b.x += b.vx;
            b.y += b.vy;

            // Screen wrap.
            if b.x < 0.0 {
                b.x += self.width;
            }
            if b.x >= self.width {
                b.x -= self.width;
            }
            if b.y < 0.0 {
                b.y += self.height;
            }
            if b.y >= self.height {
                b.y -= self.height;
            }

            // Eating: consume the nearest food within eat_radius.
            let eat_r2 = self.eat_radius * self.eat_radius;
            for (fi, f) in self.food.iter().enumerate() {
                let dx = b.x - f.x;
                let dy = b.y - f.y;
                if dx * dx + dy * dy < eat_r2 && !eaten_indices.contains(&fi) {
                    eaten_indices.push(fi);
                    b.eat_timer = 0.5;
                    break; // one food per boid per tick
                }
            }

            // Decay eat_timer.
            if b.eat_timer > 0.0 {
                b.eat_timer = (b.eat_timer - DT).max(0.0);
            }
        }

        // Remove eaten food (reverse-sorted so swap_remove is index-safe).
        eaten_indices.sort_unstable();
        for &fi in eaten_indices.iter().rev() {
            self.food.swap_remove(fi);
        }
    }

    // -- Boid flocking forces (separation, alignment, cohesion) -------------

    fn boid_flock_forces(&self, idx: usize) -> (f32, f32, f32, f32, f32, f32) {
        let boid = &self.boids[idx];
        let r2 = self.perception * self.perception;

        let mut sep_x = 0.0_f32;
        let mut sep_y = 0.0_f32;
        let mut ali_x = 0.0_f32;
        let mut ali_y = 0.0_f32;
        let mut coh_x = 0.0_f32;
        let mut coh_y = 0.0_f32;
        let mut count = 0_u32;

        for (j, other) in self.boids.iter().enumerate() {
            if j == idx {
                continue;
            }

            let (dx, dy) = Self::wrapped_delta(
                boid.x, boid.y, other.x, other.y, self.width, self.height,
            );
            let d2 = dx * dx + dy * dy;

            if d2 < r2 && d2 > 0.001 {
                let d = d2.sqrt();
                count += 1;

                sep_x += dx / d;
                sep_y += dy / d;
                ali_x += other.vx;
                ali_y += other.vy;
                coh_x -= dx; // direction *toward* the neighbour
                coh_y -= dy;
            }
        }

        if count == 0 {
            return (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        }

        let c = count as f32;

        sep_x /= c;
        sep_y /= c;
        let (sx, sy) = Self::steer(boid, sep_x, sep_y, self.max_speed, self.max_force);

        ali_x /= c;
        ali_y /= c;
        let (ax, ay) = Self::steer(boid, ali_x, ali_y, self.max_speed, self.max_force);

        coh_x /= c;
        coh_y /= c;
        let (cx, cy) = Self::steer(boid, coh_x, coh_y, self.max_speed, self.max_force);

        (sx, sy, ax, ay, cx, cy)
    }

    // -- Fear: repulsive force away from nearby predators -------------------

    fn compute_fear(&self, boid: &Boid) -> (f32, f32, bool) {
        let r2 = self.fear_radius * self.fear_radius;
        let mut fx = 0.0_f32;
        let mut fy = 0.0_f32;
        let mut feared = false;

        for pred in &self.predators {
            let (dx, dy) = Self::wrapped_delta(
                boid.x, boid.y, pred.x, pred.y, self.width, self.height,
            );
            let d2 = dx * dx + dy * dy;
            if d2 < r2 && d2 > 0.001 {
                let d = d2.sqrt();
                fx += dx / d;
                fy += dy / d;
                feared = true;
            }
        }

        if feared {
            let (sx, sy) = Self::steer(boid, fx, fy, self.max_speed, self.max_force);
            (sx, sy, true)
        } else {
            (0.0, 0.0, false)
        }
    }

    // -- Hunger: steer toward the nearest food ------------------------------

    fn compute_hunger(&self, boid: &Boid) -> (f32, f32) {
        if self.food.is_empty() {
            return (0.0, 0.0);
        }

        let mut best_dx = 0.0_f32;
        let mut best_dy = 0.0_f32;
        let mut best_d2 = f32::MAX;

        for f in &self.food {
            // Direction from boid *toward* food.
            let (dx, dy) = Self::wrapped_delta(
                f.x, f.y, boid.x, boid.y, self.width, self.height,
            );
            let d2 = dx * dx + dy * dy;
            if d2 < best_d2 {
                best_d2 = d2;
                best_dx = dx;
                best_dy = dy;
            }
        }

        Self::steer(boid, best_dx, best_dy, self.max_speed, self.max_force)
    }

    // -- Predator tick ------------------------------------------------------

    fn tick_predators(&mut self) {
        let np = self.predators.len();
        if np == 0 {
            return;
        }

        // Compute predator accelerations.
        let mut accels: Vec<(f32, f32)> = Vec::with_capacity(np);

        for i in 0..np {
            let pred = &self.predators[i];

            // Separation from other predators.
            let sep_r2: f32 = 60.0 * 60.0;
            let mut sep_x = 0.0_f32;
            let mut sep_y = 0.0_f32;
            let mut sep_count = 0_u32;

            for (j, other) in self.predators.iter().enumerate() {
                if j == i {
                    continue;
                }
                let (dx, dy) = Self::wrapped_delta(
                    pred.x, pred.y, other.x, other.y, self.width, self.height,
                );
                let d2 = dx * dx + dy * dy;
                if d2 < sep_r2 && d2 > 0.001 {
                    let d = d2.sqrt();
                    sep_x += dx / d;
                    sep_y += dy / d;
                    sep_count += 1;
                }
            }

            let (sx, sy) = if sep_count > 0 {
                sep_x /= sep_count as f32;
                sep_y /= sep_count as f32;
                Self::steer(
                    pred,
                    sep_x,
                    sep_y,
                    self.predator_speed,
                    self.predator_force,
                )
            } else {
                (0.0, 0.0)
            };

            // Chase: steer toward centre of mass of boids within perception.
            let chase_r2 = self.predator_perception * self.predator_perception;
            let mut com_x = 0.0_f32;
            let mut com_y = 0.0_f32;
            let mut chase_count = 0_u32;

            for boid in &self.boids {
                let (dx, dy) = Self::wrapped_delta(
                    boid.x, boid.y, pred.x, pred.y, self.width, self.height,
                );
                let d2 = dx * dx + dy * dy;
                if d2 < chase_r2 {
                    com_x += dx;
                    com_y += dy;
                    chase_count += 1;
                }
            }

            let (cx, cy) = if chase_count > 0 {
                com_x /= chase_count as f32;
                com_y /= chase_count as f32;
                Self::steer(
                    pred,
                    com_x,
                    com_y,
                    self.predator_speed,
                    self.predator_force,
                )
            } else {
                // No boids in range – steer toward global centre of mass.
                self.global_chase_fallback(pred)
            };

            accels.push((sx + cx * 1.5, sy + cy * 1.5));
        }

        // Integrate predators.
        for i in 0..np {
            let p = &mut self.predators[i];

            p.vx += accels[i].0;
            p.vy += accels[i].1;

            let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
            if speed > self.predator_speed {
                let s = self.predator_speed / speed;
                p.vx *= s;
                p.vy *= s;
            }

            p.x += p.vx;
            p.y += p.vy;

            if p.x < 0.0 {
                p.x += self.width;
            }
            if p.x >= self.width {
                p.x -= self.width;
            }
            if p.y < 0.0 {
                p.y += self.height;
            }
            if p.y >= self.height {
                p.y -= self.height;
            }
        }
    }

    /// When no boids are within predator perception, steer toward the global
    /// centre of mass so the predator doesn't drift aimlessly.
    fn global_chase_fallback(&self, pred: &Boid) -> (f32, f32) {
        if self.boids.is_empty() {
            return (0.0, 0.0);
        }

        let mut gx = 0.0_f32;
        let mut gy = 0.0_f32;
        for boid in &self.boids {
            let (dx, dy) = Self::wrapped_delta(
                boid.x, boid.y, pred.x, pred.y, self.width, self.height,
            );
            gx += dx;
            gy += dy;
        }
        gx /= self.boids.len() as f32;
        gy /= self.boids.len() as f32;
        Self::steer(pred, gx, gy, self.predator_speed, self.predator_force)
    }

    // -- Shared utilities ---------------------------------------------------

    /// Reynolds-style steering: desired = normalise(dir) × max_speed;
    /// steer = desired − velocity; clamp(steer, max_force).
    #[inline]
    fn steer(boid: &Boid, dx: f32, dy: f32, max_speed: f32, max_force: f32) -> (f32, f32) {
        let mag = (dx * dx + dy * dy).sqrt();
        if mag < 0.001 {
            return (0.0, 0.0);
        }

        let desired_x = dx / mag * max_speed;
        let desired_y = dy / mag * max_speed;

        let mut sx = desired_x - boid.vx;
        let mut sy = desired_y - boid.vy;

        let force = (sx * sx + sy * sy).sqrt();
        if force > max_force {
            let s = max_force / force;
            sx *= s;
            sy *= s;
        }

        (sx, sy)
    }

    /// Toroidal delta: vector from (bx, by) to (ax, ay) accounting for
    /// screen wrapping.
    #[inline]
    fn wrapped_delta(ax: f32, ay: f32, bx: f32, by: f32, w: f32, h: f32) -> (f32, f32) {
        let mut dx = ax - bx;
        let mut dy = ay - by;
        if dx > w * 0.5 {
            dx -= w;
        }
        if dx < -w * 0.5 {
            dx += w;
        }
        if dy > h * 0.5 {
            dy -= h;
        }
        if dy < -h * 0.5 {
            dy += h;
        }
        (dx, dy)
    }
}
