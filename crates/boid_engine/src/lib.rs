use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Boid – a single agent in the flock.
//
// `#[repr(C)]` guarantees the memory layout is [x, y, vx, vy] as four
// contiguous f32 values.  The JS side creates a `Float32Array` view directly
// into this memory – zero copy.
// ---------------------------------------------------------------------------
#[repr(C)]
struct Boid {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

// ---------------------------------------------------------------------------
// Flock – the simulation state exposed to JavaScript via wasm-bindgen.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct Flock {
    boids: Vec<Boid>,
    width: f32,
    height: f32,

    // Rule weights (tuneable from the UI).
    sep_weight: f32,
    ali_weight: f32,
    coh_weight: f32,

    // Physics limits.
    max_speed: f32,
    max_force: f32,
    perception: f32,
}

#[wasm_bindgen]
impl Flock {
    // -- Constructor --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(width: f32, height: f32, count: u32) -> Flock {
        let boids = Self::spawn_boids(width, height, count);

        Flock {
            boids,
            width,
            height,
            sep_weight: 1.5,
            ali_weight: 1.0,
            coh_weight: 1.0,
            max_speed: 3.0,
            max_force: 0.15,
            perception: 50.0,
        }
    }

    // -- Shared-memory pointer (the key to zero-copy rendering) -------------

    /// Returns a pointer into WASM linear memory where the boid data begins.
    /// JS constructs `new Float32Array(memory.buffer, ptr, count * 4)` to get
    /// a view of [x0, y0, vx0, vy0, x1, y1, vx1, vy1, ...].
    pub fn boids_ptr(&self) -> *const f32 {
        self.boids.as_ptr() as *const f32
    }

    /// Number of boids currently in the simulation.
    pub fn boids_count(&self) -> u32 {
        self.boids.len() as u32
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
                self.boids.push(Boid {
                    x: rng.random::<f32>() * self.width,
                    y: rng.random::<f32>() * self.height,
                    vx: (rng.random::<f32>() - 0.5) * self.max_speed,
                    vy: (rng.random::<f32>() - 0.5) * self.max_speed,
                });
            }
        } else if target < current {
            self.boids.truncate(target);
        }
    }

    // -- Simulation step ----------------------------------------------------

    /// Advance every boid by one tick using the three classic rules:
    /// separation, alignment, and cohesion.
    ///
    /// Forces are computed for all boids first, then applied, so the update
    /// order does not bias the result.  This is O(N²) which is fine for
    /// ≤ 1 500 boids.
    pub fn tick(&mut self) {
        let n = self.boids.len();
        if n == 0 {
            return;
        }

        // First pass – compute steering acceleration for every boid.
        let mut accels: Vec<(f32, f32)> = Vec::with_capacity(n);

        for i in 0..n {
            let (sx, sy, ax, ay, cx, cy) = self.forces(i);
            accels.push((
                sx * self.sep_weight + ax * self.ali_weight + cx * self.coh_weight,
                sy * self.sep_weight + ay * self.ali_weight + cy * self.coh_weight,
            ));
        }

        // Second pass – integrate velocity & position.
        for i in 0..n {
            let b = &mut self.boids[i];

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
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers (not exported to JS).
// ---------------------------------------------------------------------------
impl Flock {
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
            });
        }
        boids
    }

    /// Compute the three steering forces acting on `boids[idx]`.
    ///
    /// Returns (sep_x, sep_y, ali_x, ali_y, coh_x, coh_y) – each pair is
    /// already a clamped steering vector.
    fn forces(&self, idx: usize) -> (f32, f32, f32, f32, f32, f32) {
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

            // Toroidal distance (accounts for screen wrapping).
            let mut dx = boid.x - other.x;
            let mut dy = boid.y - other.y;
            if dx > self.width * 0.5 {
                dx -= self.width;
            }
            if dx < -self.width * 0.5 {
                dx += self.width;
            }
            if dy > self.height * 0.5 {
                dy -= self.height;
            }
            if dy < -self.height * 0.5 {
                dy += self.height;
            }

            let d2 = dx * dx + dy * dy;

            if d2 < r2 && d2 > 0.001 {
                let d = d2.sqrt();
                count += 1;

                // Separation: push away, weighted by 1/distance.
                sep_x += dx / d;
                sep_y += dy / d;

                // Alignment: accumulate neighbour velocities.
                ali_x += other.vx;
                ali_y += other.vy;

                // Cohesion: accumulate neighbour positions (relative).
                coh_x -= dx; // direction *toward* the neighbour
                coh_y -= dy;
            }
        }

        if count == 0 {
            return (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        }

        let c = count as f32;

        // Separation – average, then steer.
        sep_x /= c;
        sep_y /= c;
        let (sx, sy) = self.steer(boid, sep_x, sep_y);

        // Alignment – average velocity, then steer.
        ali_x /= c;
        ali_y /= c;
        let (ax, ay) = self.steer(boid, ali_x, ali_y);

        // Cohesion – average offset toward centre of mass, then steer.
        coh_x /= c;
        coh_y /= c;
        let (cx, cy) = self.steer(boid, coh_x, coh_y);

        (sx, sy, ax, ay, cx, cy)
    }

    /// Reynolds-style steering: desired = normalise(dir) × max_speed;
    /// steer = desired − velocity; clamp(steer, max_force).
    #[inline]
    fn steer(&self, boid: &Boid, dx: f32, dy: f32) -> (f32, f32) {
        let mag = (dx * dx + dy * dy).sqrt();
        if mag < 0.001 {
            return (0.0, 0.0);
        }

        let desired_x = dx / mag * self.max_speed;
        let desired_y = dy / mag * self.max_speed;

        let mut sx = desired_x - boid.vx;
        let mut sy = desired_y - boid.vy;

        let force = (sx * sx + sy * sy).sqrt();
        if force > self.max_force {
            let s = self.max_force / force;
            sx *= s;
            sy *= s;
        }

        (sx, sy)
    }
}
