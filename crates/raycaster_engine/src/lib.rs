use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Map constants
// ---------------------------------------------------------------------------
const MAP_W: usize = 64;
const MAP_H: usize = 64;
const MAP_SIZE: usize = MAP_W * MAP_H;

// Minimap
const MINIMAP_SCALE: usize = 2; // each map cell → 2×2 display pixels
const MINIMAP_W: usize = MAP_W * MINIMAP_SCALE;
const MINIMAP_H: usize = MAP_H * MINIMAP_SCALE;
const MINIMAP_PAD: usize = 4; // pixels of padding from corner

// ---------------------------------------------------------------------------
// A hand-crafted maze with outer walls and interior corridors.
// 1 = wall, 0 = empty.
// ---------------------------------------------------------------------------
fn generate_map() -> [u8; MAP_SIZE] {
    let mut m = [0u8; MAP_SIZE];

    // Outer walls
    for x in 0..MAP_W {
        m[x] = 1;                          // top
        m[(MAP_H - 1) * MAP_W + x] = 1;   // bottom
    }
    for y in 0..MAP_H {
        m[y * MAP_W] = 1;                  // left
        m[y * MAP_W + MAP_W - 1] = 1;      // right
    }

    // Interior walls — a maze of rooms and corridors
    let walls: &[(usize, usize, usize, usize)] = &[
        // (x_start, y_start, x_end, y_end) — inclusive ranges
        // Horizontal walls
        (1, 8, 20, 8),
        (25, 8, 40, 8),
        (45, 8, 62, 8),
        (1, 16, 12, 16),
        (18, 16, 30, 16),
        (36, 16, 50, 16),
        (55, 16, 62, 16),
        (1, 24, 15, 24),
        (22, 24, 42, 24),
        (48, 24, 62, 24),
        (1, 32, 10, 32),
        (16, 32, 28, 32),
        (34, 32, 48, 32),
        (54, 32, 62, 32),
        (1, 40, 18, 40),
        (24, 40, 38, 40),
        (44, 40, 62, 40),
        (1, 48, 14, 48),
        (20, 48, 34, 48),
        (40, 48, 52, 48),
        (58, 48, 62, 48),
        (1, 56, 8, 56),
        (14, 56, 30, 56),
        (36, 56, 50, 56),
        (56, 56, 62, 56),

        // Vertical walls
        (10, 1, 10, 6),
        (20, 1, 20, 6),
        (32, 1, 32, 6),
        (50, 1, 50, 6),
        (8, 9, 8, 14),
        (24, 9, 24, 14),
        (40, 9, 40, 14),
        (56, 9, 56, 14),
        (12, 17, 12, 22),
        (30, 17, 30, 22),
        (48, 17, 48, 22),
        (10, 25, 10, 30),
        (22, 25, 22, 30),
        (36, 25, 36, 30),
        (54, 25, 54, 30),
        (16, 33, 16, 38),
        (28, 33, 28, 38),
        (44, 33, 44, 38),
        (8, 41, 8, 46),
        (20, 41, 20, 46),
        (38, 41, 38, 46),
        (52, 41, 52, 46),
        (14, 49, 14, 54),
        (34, 49, 34, 54),
        (50, 49, 50, 54),
    ];

    for &(x1, y1, x2, y2) in walls {
        if y1 == y2 {
            // Horizontal wall
            for x in x1..=x2.min(MAP_W - 1) {
                m[y1 * MAP_W + x] = 1;
            }
        } else if x1 == x2 {
            // Vertical wall
            for y in y1..=y2.min(MAP_H - 1) {
                m[y * MAP_W + x1] = 1;
            }
        }
    }

    // A few thick pillars (2×2 blocks) scattered around
    let pillars: &[(usize, usize)] = &[
        (15, 12), (35, 12), (55, 12),
        (5, 28), (25, 28), (45, 28),
        (15, 44), (35, 44), (55, 44),
    ];
    for &(px, py) in pillars {
        for dy in 0..2 {
            for dx in 0..2 {
                let x = px + dx;
                let y = py + dy;
                if x < MAP_W && y < MAP_H {
                    m[y * MAP_W + x] = 1;
                }
            }
        }
    }

    m
}

// ---------------------------------------------------------------------------
// World — the raycaster state.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct World {
    map: [u8; MAP_SIZE],

    // Player camera
    pos_x: f64,
    pos_y: f64,
    dir_x: f64,
    dir_y: f64,
    plane_x: f64,
    plane_y: f64,

    // Output buffer
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[wasm_bindgen]
impl World {
    // -- Constructor --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> World {
        let map = generate_map();

        // Start the player in an open area, looking east
        World {
            map,
            pos_x: 3.5,
            pos_y: 3.5,
            dir_x: 1.0,
            dir_y: 0.0,
            plane_x: 0.0,
            plane_y: 0.66, // FOV ≈ 66°
            width,
            height,
            pixels: vec![0u8; (width * height * 4) as usize],
        }
    }

    // -- Shared-memory pointers --------------------------------------------

    pub fn pixels_ptr(&self) -> *const u8 {
        self.pixels.as_ptr()
    }

    pub fn pixels_len(&self) -> usize {
        self.pixels.len()
    }

    // -- FOV control -------------------------------------------------------

    /// Set the camera plane length (controls FOV). Default is 0.66.
    pub fn set_fov(&mut self, fov_len: f64) {
        // Normalise the plane vector to the requested length
        let current_len = (self.plane_x * self.plane_x + self.plane_y * self.plane_y).sqrt();
        if current_len > 1e-9 {
            let scale = fov_len / current_len;
            self.plane_x *= scale;
            self.plane_y *= scale;
        }
    }

    // -- Movement ----------------------------------------------------------

    pub fn move_forward(&mut self, speed: f64) {
        let nx = self.pos_x + self.dir_x * speed;
        let ny = self.pos_y + self.dir_y * speed;
        // Slide along walls: check x and y independently
        if is_open(&self.map, nx, self.pos_y) {
            self.pos_x = nx;
        }
        if is_open(&self.map, self.pos_x, ny) {
            self.pos_y = ny;
        }
    }

    pub fn move_backward(&mut self, speed: f64) {
        let nx = self.pos_x - self.dir_x * speed;
        let ny = self.pos_y - self.dir_y * speed;
        if is_open(&self.map, nx, self.pos_y) {
            self.pos_x = nx;
        }
        if is_open(&self.map, self.pos_x, ny) {
            self.pos_y = ny;
        }
    }

    pub fn strafe_left(&mut self, speed: f64) {
        let nx = self.pos_x - self.plane_x * speed;
        let ny = self.pos_y - self.plane_y * speed;
        if is_open(&self.map, nx, self.pos_y) {
            self.pos_x = nx;
        }
        if is_open(&self.map, self.pos_x, ny) {
            self.pos_y = ny;
        }
    }

    pub fn strafe_right(&mut self, speed: f64) {
        let nx = self.pos_x + self.plane_x * speed;
        let ny = self.pos_y + self.plane_y * speed;
        if is_open(&self.map, nx, self.pos_y) {
            self.pos_x = nx;
        }
        if is_open(&self.map, self.pos_x, ny) {
            self.pos_y = ny;
        }
    }

    pub fn rotate_left(&mut self, speed: f64) {
        let cos = speed.cos();
        let sin = speed.sin();
        let old_dir_x = self.dir_x;
        self.dir_x = self.dir_x * cos - self.dir_y * sin;
        self.dir_y = old_dir_x * sin + self.dir_y * cos;
        let old_plane_x = self.plane_x;
        self.plane_x = self.plane_x * cos - self.plane_y * sin;
        self.plane_y = old_plane_x * sin + self.plane_y * cos;
    }

    pub fn rotate_right(&mut self, speed: f64) {
        let cos = speed.cos();
        let sin = (-speed).sin(); // negative angle
        let old_dir_x = self.dir_x;
        self.dir_x = self.dir_x * cos - self.dir_y * sin;
        self.dir_y = old_dir_x * sin + self.dir_y * cos;
        let old_plane_x = self.plane_x;
        self.plane_x = self.plane_x * cos - self.plane_y * sin;
        self.plane_y = old_plane_x * sin + self.plane_y * cos;
    }

    // -- Raycasting render -------------------------------------------------

    /// Render the scene into the pixel buffer.
    ///
    /// `time` — elapsed seconds (drives the psychedelic colour animation).
    /// `trippiness` — speed multiplier for the colour pulse (0.0 = static).
    pub fn render(&mut self, time: f64, trippiness: f64) {
        let w = self.width as usize;
        let h = self.height as usize;

        // ── Cast one ray per column ──────────────────────────────────────
        for x in 0..w {
            // Camera-space x coordinate: -1 (left) to +1 (right)
            let camera_x: f64 = 2.0 * x as f64 / w as f64 - 1.0;

            // Ray direction
            let ray_dir_x = self.dir_x + self.plane_x * camera_x;
            let ray_dir_y = self.dir_y + self.plane_y * camera_x;

            // Current map cell
            let mut map_x = self.pos_x as i32;
            let mut map_y = self.pos_y as i32;

            // Delta distance: length of ray from one x/y-side to next
            let delta_dist_x = if ray_dir_x == 0.0 {
                1e30
            } else {
                (1.0 / ray_dir_x).abs()
            };
            let delta_dist_y = if ray_dir_y == 0.0 {
                1e30
            } else {
                (1.0 / ray_dir_y).abs()
            };

            // Step direction and initial side distance
            let (step_x, mut side_dist_x) = if ray_dir_x < 0.0 {
                (-1i32, (self.pos_x - map_x as f64) * delta_dist_x)
            } else {
                (1i32, (map_x as f64 + 1.0 - self.pos_x) * delta_dist_x)
            };

            let (step_y, mut side_dist_y) = if ray_dir_y < 0.0 {
                (-1i32, (self.pos_y - map_y as f64) * delta_dist_y)
            } else {
                (1i32, (map_y as f64 + 1.0 - self.pos_y) * delta_dist_y)
            };

            // ── DDA ──────────────────────────────────────────────────────
            let mut side: u8 = 0; // 0 = x-side, 1 = y-side
            let max_steps = 128;
            let mut hit = false;

            for _ in 0..max_steps {
                if side_dist_x < side_dist_y {
                    side_dist_x += delta_dist_x;
                    map_x += step_x;
                    side = 0;
                } else {
                    side_dist_y += delta_dist_y;
                    map_y += step_y;
                    side = 1;
                }

                // Bounds check
                if map_x < 0 || map_x >= MAP_W as i32 || map_y < 0 || map_y >= MAP_H as i32 {
                    break;
                }

                if self.map[map_y as usize * MAP_W + map_x as usize] != 0 {
                    hit = true;
                    break;
                }
            }

            if !hit {
                // No wall hit — draw ceiling/floor only
                self.draw_column_no_wall(x, h);
                continue;
            }

            // ── Perpendicular wall distance (avoids fisheye) ─────────────
            let perp_wall_dist = if side == 0 {
                side_dist_x - delta_dist_x
            } else {
                side_dist_y - delta_dist_y
            };

            // ── Line height on screen ────────────────────────────────────
            let line_height = if perp_wall_dist > 0.0 {
                (h as f64 / perp_wall_dist) as i32
            } else {
                h as i32
            };

            let draw_start = (-(line_height / 2) + h as i32 / 2).max(0) as usize;
            let draw_end = ((line_height / 2) + h as i32 / 2).min(h as i32) as usize;

            // ── Calculate exact wall-hit coordinate for texturing ────────
            let wall_x = if side == 0 {
                self.pos_y + perp_wall_dist * ray_dir_y
            } else {
                self.pos_x + perp_wall_dist * ray_dir_x
            };
            let wall_x_frac = wall_x - wall_x.floor();

            // ── Psychedelic wall colour ──────────────────────────────────
            let (wr, wg, wb) = psychedelic_color(
                wall_x_frac,
                map_x as f64,
                map_y as f64,
                time,
                trippiness,
            );

            // Darken y-side walls for depth perception
            let (wr, wg, wb) = if side == 1 {
                (wr / 2, wg / 2, wb / 2)
            } else {
                (wr, wg, wb)
            };

            // ── Draw the vertical stripe ─────────────────────────────────
            // Ceiling (dark grey)
            for y in 0..draw_start {
                let p = (y * w + x) * 4;
                self.pixels[p] = 0x22;
                self.pixels[p + 1] = 0x22;
                self.pixels[p + 2] = 0x28;
                self.pixels[p + 3] = 0xFF;
            }

            // Wall
            for y in draw_start..draw_end {
                let p = (y * w + x) * 4;
                self.pixels[p] = wr;
                self.pixels[p + 1] = wg;
                self.pixels[p + 2] = wb;
                self.pixels[p + 3] = 0xFF;
            }

            // Floor (dark brown)
            for y in draw_end..h {
                let p = (y * w + x) * 4;
                self.pixels[p] = 0x30;
                self.pixels[p + 1] = 0x24;
                self.pixels[p + 2] = 0x18;
                self.pixels[p + 3] = 0xFF;
            }
        }

        // ── Minimap overlay ──────────────────────────────────────────────
        self.draw_minimap(w);
    }
}

// ---------------------------------------------------------------------------
// Bounds-checked map lookup — returns true when (x, y) is inside the map
// and the cell is empty (0).  Out-of-bounds coordinates are treated as walls.
// ---------------------------------------------------------------------------
fn is_open(map: &[u8; MAP_SIZE], x: f64, y: f64) -> bool {
    let xi = x as usize; // negative f64 saturates to 0
    let yi = y as usize;
    xi < MAP_W && yi < MAP_H && map[yi * MAP_W + xi] == 0
}

// ---------------------------------------------------------------------------
// Psychedelic colour: uses sin waves to create shifting hues.
// ---------------------------------------------------------------------------
fn psychedelic_color(
    wall_frac: f64,
    map_x: f64,
    map_y: f64,
    time: f64,
    trippiness: f64,
) -> (u8, u8, u8) {
    let t = time * trippiness;
    let spatial = map_x * 0.3 + map_y * 0.5 + wall_frac * 3.0;

    let r = ((spatial + t * 1.0).sin() * 0.5 + 0.5) * 255.0;
    let g = ((spatial * 1.3 + t * 1.3 + 2.094).sin() * 0.5 + 0.5) * 255.0;
    let b = ((spatial * 0.7 + t * 0.7 + 4.189).sin() * 0.5 + 0.5) * 255.0;

    (r as u8, g as u8, b as u8)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------
impl World {
    /// Draw a column that missed every wall (ceiling + floor only).
    fn draw_column_no_wall(&mut self, x: usize, h: usize) {
        let w = self.width as usize;
        let half = h / 2;
        for y in 0..half {
            let p = (y * w + x) * 4;
            self.pixels[p] = 0x22;
            self.pixels[p + 1] = 0x22;
            self.pixels[p + 2] = 0x28;
            self.pixels[p + 3] = 0xFF;
        }
        for y in half..h {
            let p = (y * w + x) * 4;
            self.pixels[p] = 0x30;
            self.pixels[p + 1] = 0x24;
            self.pixels[p + 2] = 0x18;
            self.pixels[p + 3] = 0xFF;
        }
    }

    /// Draw a top-down minimap in the top-left corner of the pixel buffer.
    fn draw_minimap(&mut self, buf_w: usize) {
        let ox = MINIMAP_PAD;
        let oy = MINIMAP_PAD;

        for my in 0..MAP_H {
            for mx in 0..MAP_W {
                let is_wall = self.map[my * MAP_W + mx] != 0;

                let (r, g, b, a) = if is_wall {
                    (0x66u8, 0x66, 0x88, 0xCC)
                } else {
                    (0x11, 0x11, 0x18, 0xAA)
                };

                // Write the MINIMAP_SCALE × MINIMAP_SCALE block
                for sy in 0..MINIMAP_SCALE {
                    for sx in 0..MINIMAP_SCALE {
                        let px = ox + mx * MINIMAP_SCALE + sx;
                        let py = oy + my * MINIMAP_SCALE + sy;
                        if px < buf_w && py < self.height as usize {
                            let p = (py * buf_w + px) * 4;
                            // Alpha-blend over existing pixel
                            let alpha = a as u16;
                            let inv = 255 - alpha;
                            self.pixels[p] =
                                ((r as u16 * alpha + self.pixels[p] as u16 * inv) / 255) as u8;
                            self.pixels[p + 1] =
                                ((g as u16 * alpha + self.pixels[p + 1] as u16 * inv) / 255) as u8;
                            self.pixels[p + 2] =
                                ((b as u16 * alpha + self.pixels[p + 2] as u16 * inv) / 255) as u8;
                            self.pixels[p + 3] = 0xFF;
                        }
                    }
                }
            }
        }

        // ── Player dot (bright green, 2×2) ───────────────────────────────
        let ppx = ox + (self.pos_x * MINIMAP_SCALE as f64) as usize;
        let ppy = oy + (self.pos_y * MINIMAP_SCALE as f64) as usize;
        for dy in 0..2usize {
            for dx in 0..2usize {
                let px = ppx + dx;
                let py = ppy + dy;
                if px < buf_w && py < self.height as usize {
                    let p = (py * buf_w + px) * 4;
                    self.pixels[p] = 0x00;
                    self.pixels[p + 1] = 0xFF;
                    self.pixels[p + 2] = 0x00;
                    self.pixels[p + 3] = 0xFF;
                }
            }
        }

        // ── Direction line (4 pixels along dir vector) ───────────────────
        for i in 1..=4 {
            let lx = ppx as f64 + self.dir_x * i as f64 * MINIMAP_SCALE as f64 * 0.6;
            let ly = ppy as f64 + self.dir_y * i as f64 * MINIMAP_SCALE as f64 * 0.6;
            let lx = lx as usize;
            let ly = ly as usize;
            if lx < buf_w && ly < self.height as usize {
                let p = (ly * buf_w + lx) * 4;
                self.pixels[p] = 0xFF;
                self.pixels[p + 1] = 0xFF;
                self.pixels[p + 2] = 0x00;
                self.pixels[p + 3] = 0xFF;
            }
        }

        // ── Minimap border ───────────────────────────────────────────────
        let border_r = ox + MINIMAP_W;
        let border_b = oy + MINIMAP_H;
        for px in ox..border_r.min(buf_w) {
            // Top border
            if oy > 0 && oy - 1 < self.height as usize {
                self.set_pixel_if(buf_w, px, oy.saturating_sub(1), 0x44, 0x44, 0x55);
            }
            // Bottom border
            if border_b < self.height as usize {
                self.set_pixel_if(buf_w, px, border_b, 0x44, 0x44, 0x55);
            }
        }
        for py in oy..border_b.min(self.height as usize) {
            // Left border
            if ox > 0 {
                self.set_pixel_if(buf_w, ox.saturating_sub(1), py, 0x44, 0x44, 0x55);
            }
            // Right border
            if border_r < buf_w {
                self.set_pixel_if(buf_w, border_r, py, 0x44, 0x44, 0x55);
            }
        }
    }

    #[inline]
    fn set_pixel_if(&mut self, buf_w: usize, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x < buf_w && y < self.height as usize {
            let p = (y * buf_w + x) * 4;
            self.pixels[p] = r;
            self.pixels[p + 1] = g;
            self.pixels[p + 2] = b;
            self.pixels[p + 3] = 0xFF;
        }
    }
}
