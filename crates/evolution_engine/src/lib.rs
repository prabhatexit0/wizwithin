use rand::Rng;
use std::f32::consts::PI;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DT: f32 = 1.0 / 60.0;
const GRAVITY: f32 = 300.0;
const GROUND_Y: f32 = 450.0;
const SPRING_K: f32 = 400.0;
const DAMPING: f32 = 0.998;
const GROUND_FRICTION: f32 = 0.9;

const START_X: f32 = 60.0;
const START_Y: f32 = GROUND_Y - 30.0;

const CREATURE_W: f32 = 25.0;
const CREATURE_H: f32 = 25.0;

const POINTS_PER: usize = 4;
const MUSCLES_PER: usize = 6;
const DNA_PER_MUSCLE: usize = 3; // amplitude, frequency, phase
const DNA_SIZE: usize = MUSCLES_PER * DNA_PER_MUSCLE;

const GENERATION_TICKS: u32 = 600; // 10 seconds at 60 fps

// Muscle topology within each creature.
//
//   0---1        edges: 0-1 top, 2-3 bottom, 0-2 left, 1-3 right
//   |\ /|        diags: 0-3 backslash, 1-2 slash
//   | X |
//   |/ \|
//   2---3
const MUSCLE_A: [usize; MUSCLES_PER] = [0, 2, 0, 1, 0, 1];
const MUSCLE_B: [usize; MUSCLES_PER] = [1, 3, 2, 3, 3, 2];

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

// ---------------------------------------------------------------------------
// Muscle
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Muscle {
    a: usize,
    b: usize,
    rest_length: f32,
    amplitude: f32,
    frequency: f32,
    phase: f32,
}

impl Muscle {
    fn target_length(&self, time: f32) -> f32 {
        self.rest_length * (1.0 + self.amplitude * (time * self.frequency * 2.0 * PI + self.phase).sin())
    }
}

// ---------------------------------------------------------------------------
// Creature
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Creature {
    points: [Point; POINTS_PER],
    muscles: [Muscle; MUSCLES_PER],
    dna: [f32; DNA_SIZE],
    max_x: f32,
}

impl Creature {
    fn new(dna: [f32; DNA_SIZE]) -> Self {
        let points = [
            Point { x: START_X, y: START_Y, vx: 0.0, vy: 0.0 },
            Point { x: START_X + CREATURE_W, y: START_Y, vx: 0.0, vy: 0.0 },
            Point { x: START_X, y: START_Y + CREATURE_H, vx: 0.0, vy: 0.0 },
            Point { x: START_X + CREATURE_W, y: START_Y + CREATURE_H, vx: 0.0, vy: 0.0 },
        ];

        let muscles = Self::build_muscles(&points, &dna);

        Creature { points, muscles, dna, max_x: START_X }
    }

    fn build_muscles(pts: &[Point; POINTS_PER], dna: &[f32; DNA_SIZE]) -> [Muscle; MUSCLES_PER] {
        let mut muscles = [Muscle {
            a: 0, b: 0, rest_length: 0.0,
            amplitude: 0.0, frequency: 0.0, phase: 0.0,
        }; MUSCLES_PER];

        for i in 0..MUSCLES_PER {
            let a = MUSCLE_A[i];
            let b = MUSCLE_B[i];
            let dx = pts[b].x - pts[a].x;
            let dy = pts[b].y - pts[a].y;
            muscles[i] = Muscle {
                a,
                b,
                rest_length: (dx * dx + dy * dy).sqrt(),
                amplitude: dna[i * DNA_PER_MUSCLE],
                frequency: dna[i * DNA_PER_MUSCLE + 1],
                phase: dna[i * DNA_PER_MUSCLE + 2],
            };
        }
        muscles
    }

    fn center_x(&self) -> f32 {
        (self.points[0].x + self.points[1].x + self.points[2].x + self.points[3].x) * 0.25
    }

}

// ---------------------------------------------------------------------------
// Simulation – the top-level struct exposed to JavaScript
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct Simulation {
    creatures: Vec<Creature>,
    time: f32,
    generation: u32,
    ticks_in_gen: u32,
    record_distance: f32,
    pop_size: usize,

    // Flat rendering buffers (zero-copy via shared WASM memory).
    point_buf: Vec<f32>,      // [x, y, x, y, ...] for every point of every creature
    muscle_idx_buf: Vec<u32>, // topology template: [a0, b0, a1, b1, ...]
    best_idx: u32,
}

// ---------------------------------------------------------------------------
// Public WASM API
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl Simulation {
    #[wasm_bindgen(constructor)]
    pub fn new(pop_size: u32) -> Simulation {
        let pop = pop_size.max(4) as usize;
        let mut rng = rand::rng();

        let creatures: Vec<Creature> = (0..pop)
            .map(|_| Creature::new(random_dna(&mut rng)))
            .collect();

        let point_buf = vec![0.0_f32; pop * POINTS_PER * 2];

        let muscle_idx_buf: Vec<u32> = (0..MUSCLES_PER)
            .flat_map(|i| [MUSCLE_A[i] as u32, MUSCLE_B[i] as u32])
            .collect();

        let mut sim = Simulation {
            creatures,
            time: 0.0,
            generation: 1,
            ticks_in_gen: 0,
            record_distance: 0.0,
            pop_size: pop,
            point_buf,
            muscle_idx_buf,
            best_idx: 0,
        };
        sim.update_buffers();
        sim
    }

    /// Advance the simulation by `steps` physics frames.
    /// Automatically triggers `evolve()` when the generation timer expires.
    pub fn tick(&mut self, steps: u32) {
        for _ in 0..steps {
            self.step_physics();
            self.ticks_in_gen += 1;
            if self.ticks_in_gen >= GENERATION_TICKS {
                self.evolve();
            }
        }
        self.update_buffers();
    }

    // -- Rendering pointers -------------------------------------------------

    pub fn points_ptr(&self) -> *const f32 {
        self.point_buf.as_ptr()
    }

    pub fn points_len(&self) -> u32 {
        self.point_buf.len() as u32
    }

    pub fn points_per_creature(&self) -> u32 {
        POINTS_PER as u32
    }

    pub fn creature_count(&self) -> u32 {
        self.pop_size as u32
    }

    pub fn muscle_indices_ptr(&self) -> *const u32 {
        self.muscle_idx_buf.as_ptr()
    }

    pub fn muscle_count(&self) -> u32 {
        MUSCLES_PER as u32
    }

    // -- Stats --------------------------------------------------------------

    pub fn best_idx(&self) -> u32 {
        self.best_idx
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn best_distance(&self) -> f32 {
        self.creatures
            .get(self.best_idx as usize)
            .map(|c| (c.max_x - START_X).max(0.0))
            .unwrap_or(0.0)
    }

    pub fn record_distance(&self) -> f32 {
        self.record_distance
    }

    pub fn gen_progress(&self) -> f32 {
        self.ticks_in_gen as f32 / GENERATION_TICKS as f32
    }

    pub fn ground_y(&self) -> f32 {
        GROUND_Y
    }

    pub fn start_x(&self) -> f32 {
        START_X
    }

    /// Hard reset: new random population, generation 1.
    pub fn reset(&mut self) {
        let mut rng = rand::rng();
        self.creatures = (0..self.pop_size)
            .map(|_| Creature::new(random_dna(&mut rng)))
            .collect();
        self.time = 0.0;
        self.generation = 1;
        self.ticks_in_gen = 0;
        self.record_distance = 0.0;
        self.update_buffers();
    }
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

impl Simulation {
    fn step_physics(&mut self) {
        self.time += DT;
        let time = self.time;

        for creature in &mut self.creatures {
            // 1. Muscle spring forces.
            for mi in 0..MUSCLES_PER {
                let target = creature.muscles[mi].target_length(time);
                let a = creature.muscles[mi].a;
                let b = creature.muscles[mi].b;

                let dx = creature.points[b].x - creature.points[a].x;
                let dy = creature.points[b].y - creature.points[a].y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.001);
                let force = SPRING_K * (dist - target) / dist;

                let fx = force * dx * DT;
                let fy = force * dy * DT;

                creature.points[a].vx += fx;
                creature.points[a].vy += fy;
                creature.points[b].vx -= fx;
                creature.points[b].vy -= fy;
            }

            // 2. Gravity + damping + integrate + ground collision.
            for p in &mut creature.points {
                p.vy += GRAVITY * DT;

                p.vx *= DAMPING;
                p.vy *= DAMPING;

                p.x += p.vx * DT;
                p.y += p.vy * DT;

                // Ground
                if p.y >= GROUND_Y {
                    p.y = GROUND_Y;
                    if p.vy > 0.0 {
                        p.vy *= -0.05;
                    }
                    p.vx *= GROUND_FRICTION;
                }

                // Ceiling
                if p.y < 0.0 {
                    p.y = 0.0;
                    if p.vy < 0.0 {
                        p.vy = 0.0;
                    }
                }
            }

            // 3. Update fitness.
            let cx = creature.center_x();
            if cx > creature.max_x {
                creature.max_x = cx;
            }
        }
    }

    fn evolve(&mut self) {
        // Sort by fitness (max_x), descending.
        self.creatures
            .sort_by(|a, b| b.max_x.partial_cmp(&a.max_x).unwrap_or(std::cmp::Ordering::Equal));

        // Update record.
        let best_dist = (self.creatures[0].max_x - START_X).max(0.0);
        if best_dist > self.record_distance {
            self.record_distance = best_dist;
        }

        // Keep top 50%, clone + mutate for bottom 50%.
        let half = self.pop_size / 2;
        let survivor_dna: Vec<[f32; DNA_SIZE]> =
            self.creatures[..half].iter().map(|c| c.dna).collect();

        let mut rng = rand::rng();
        let mut new_creatures: Vec<Creature> = Vec::with_capacity(self.pop_size);

        // Survivors (reset to start).
        for dna in &survivor_dna {
            new_creatures.push(Creature::new(*dna));
        }

        // Children (mutated clones).
        for i in 0..(self.pop_size - half) {
            let parent = survivor_dna[i % half];
            let child = mutate_dna(&parent, &mut rng);
            new_creatures.push(Creature::new(child));
        }

        self.creatures = new_creatures;
        self.generation += 1;
        self.ticks_in_gen = 0;
        self.time = 0.0;
    }

    fn update_buffers(&mut self) {
        // Find the creature currently furthest right.
        let mut best_i = 0;
        let mut best_x = f32::NEG_INFINITY;
        for (i, c) in self.creatures.iter().enumerate() {
            let cx = c.center_x();
            if cx > best_x {
                best_x = cx;
                best_i = i;
            }
        }
        self.best_idx = best_i as u32;

        // Pack all point positions into the flat buffer.
        for (ci, creature) in self.creatures.iter().enumerate() {
            let base = ci * POINTS_PER * 2;
            for (pi, pt) in creature.points.iter().enumerate() {
                self.point_buf[base + pi * 2] = pt.x;
                self.point_buf[base + pi * 2 + 1] = pt.y;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DNA helpers
// ---------------------------------------------------------------------------

fn random_dna(rng: &mut impl Rng) -> [f32; DNA_SIZE] {
    let mut dna = [0.0_f32; DNA_SIZE];
    for i in 0..DNA_SIZE {
        dna[i] = match i % DNA_PER_MUSCLE {
            0 => rng.random::<f32>() * 0.3 + 0.05,        // amplitude  [0.05, 0.35]
            1 => rng.random::<f32>() * 4.0 + 1.0,          // frequency  [1.0, 5.0]
            2 => rng.random::<f32>() * 2.0 * PI,            // phase      [0, 2π]
            _ => unreachable!(),
        };
    }
    dna
}

fn mutate_dna(parent: &[f32; DNA_SIZE], rng: &mut impl Rng) -> [f32; DNA_SIZE] {
    let mut child = *parent;
    for i in 0..DNA_SIZE {
        match i % DNA_PER_MUSCLE {
            0 => {
                // amplitude: small perturbation, clamp [0, 0.5]
                child[i] += (rng.random::<f32>() - 0.5) * 0.1;
                child[i] = child[i].clamp(0.0, 0.5);
            }
            1 => {
                // frequency: perturbation, clamp [0.5, 8]
                child[i] += (rng.random::<f32>() - 0.5) * 1.0;
                child[i] = child[i].clamp(0.5, 8.0);
            }
            2 => {
                // phase: perturbation, wrap [0, 2π]
                child[i] += (rng.random::<f32>() - 0.5) * 0.6;
                child[i] = child[i].rem_euclid(2.0 * PI);
            }
            _ => unreachable!(),
        }
    }
    child
}
