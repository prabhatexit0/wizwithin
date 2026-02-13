use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Vec2
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Default)]
struct Vec2 {
    x: f32,
    y: f32,
}

impl Vec2 {
    fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y
    }
    fn len_sq(self) -> f32 {
        self.dot(self)
    }
    fn len(self) -> f32 {
        self.len_sq().sqrt()
    }
    fn normalized(self) -> Self {
        let l = self.len();
        if l < 1e-8 {
            Self::new(0.0, 0.0)
        } else {
            Self::new(self.x / l, self.y / l)
        }
    }
    fn cross_scalar(self, o: Self) -> f32 {
        self.x * o.y - self.y * o.x
    }
}

impl core::ops::Add for Vec2 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }
}
impl core::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}
impl core::ops::Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}
impl core::ops::Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------
#[derive(Clone, Copy)]
enum Shape {
    Circle { radius: f32 },
    Rect { half_w: f32, half_h: f32 },
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Body {
    pos: Vec2,
    vel: Vec2,
    angle: f32,
    angular_vel: f32,
    mass: f32,
    inv_mass: f32,
    inertia: f32,
    inv_inertia: f32,
    restitution: f32,
    friction: f32,
    shape: Shape,
    is_static: bool,
    // Spring drag (mouse interaction)
    drag_target: Option<Vec2>,
}

impl Body {
    fn new(x: f32, y: f32, shape: Shape, mass: f32, restitution: f32, friction: f32) -> Self {
        let is_static = mass <= 0.0;
        let actual_mass = if is_static { 0.0 } else { mass };
        let inv_mass = if is_static { 0.0 } else { 1.0 / actual_mass };

        let inertia = if is_static {
            0.0
        } else {
            match shape {
                Shape::Circle { radius } => 0.5 * actual_mass * radius * radius,
                Shape::Rect { half_w, half_h } => {
                    let w = half_w * 2.0;
                    let h = half_h * 2.0;
                    actual_mass * (w * w + h * h) / 12.0
                }
            }
        };
        let inv_inertia = if inertia > 0.0 { 1.0 / inertia } else { 0.0 };

        Self {
            pos: Vec2::new(x, y),
            vel: Vec2::default(),
            angle: 0.0,
            angular_vel: 0.0,
            mass: actual_mass,
            inv_mass,
            inertia,
            inv_inertia,
            restitution,
            friction,
            shape,
            is_static,
            drag_target: None,
        }
    }

    fn aabb(&self) -> (f32, f32, f32, f32) {
        match self.shape {
            Shape::Circle { radius } => (
                self.pos.x - radius,
                self.pos.y - radius,
                self.pos.x + radius,
                self.pos.y + radius,
            ),
            Shape::Rect { half_w, half_h } => {
                // Compute rotated AABB
                let c = self.angle.cos();
                let s = self.angle.sin();
                let ex = (c * half_w).abs() + (s * half_h).abs();
                let ey = (s * half_w).abs() + (c * half_h).abs();
                (
                    self.pos.x - ex,
                    self.pos.y - ey,
                    self.pos.x + ex,
                    self.pos.y + ey,
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Collision manifold
// ---------------------------------------------------------------------------
struct Contact {
    a: usize,
    b: usize,
    normal: Vec2,   // Points from A to B
    depth: f32,
    contact_point: Vec2,
}

// ---------------------------------------------------------------------------
// Narrow-phase: Circle vs Circle
// ---------------------------------------------------------------------------
fn collide_circle_circle(a: &Body, ra: f32, b: &Body, rb: f32, ia: usize, ib: usize) -> Option<Contact> {
    let d = b.pos - a.pos;
    let dist_sq = d.len_sq();
    let sum_r = ra + rb;
    if dist_sq >= sum_r * sum_r || dist_sq < 1e-12 {
        return None;
    }
    let dist = dist_sq.sqrt();
    let normal = d * (1.0 / dist);
    let depth = sum_r - dist;
    let contact_point = a.pos + normal * (ra - depth * 0.5);
    Some(Contact { a: ia, b: ib, normal, depth, contact_point })
}

// ---------------------------------------------------------------------------
// Narrow-phase: Circle vs OBB
// ---------------------------------------------------------------------------
fn collide_circle_rect(
    circle: &Body, radius: f32,
    rect: &Body, half_w: f32, half_h: f32,
    ci: usize, ri: usize, flip: bool,
) -> Option<Contact> {
    // Transform circle center into rect's local space
    let d = circle.pos - rect.pos;
    let c = rect.angle.cos();
    let s = rect.angle.sin();
    let local_x = d.x * c + d.y * s;
    let local_y = -d.x * s + d.y * c;

    // Clamp to rect extents to find closest point
    let cx = local_x.clamp(-half_w, half_w);
    let cy = local_y.clamp(-half_h, half_h);

    let dx = local_x - cx;
    let dy = local_y - cy;
    let dist_sq = dx * dx + dy * dy;

    if dist_sq >= radius * radius && dist_sq > 1e-12 {
        return None;
    }

    // Circle center is inside the rect
    if dist_sq < 1e-12 {
        // Push along the shortest axis
        let pen_x = half_w - local_x.abs();
        let pen_y = half_h - local_y.abs();
        let (local_normal, depth) = if pen_x < pen_y {
            (Vec2::new(if local_x < 0.0 { -1.0 } else { 1.0 }, 0.0), pen_x + radius)
        } else {
            (Vec2::new(0.0, if local_y < 0.0 { -1.0 } else { 1.0 }), pen_y + radius)
        };
        // Transform to world space — points from rect surface toward circle
        let phys_normal = Vec2::new(
            local_normal.x * c - local_normal.y * s,
            local_normal.x * s + local_normal.y * c,
        );
        // Contact point: on circle surface toward rect
        let contact_point = circle.pos - phys_normal * (radius - depth * 0.5);
        // Flip for Contact convention: normal must point from Contact.a to Contact.b
        // phys_normal points rect→circle; non-flip: a=circle,b=rect so negate; flip: a=rect,b=circle so keep
        let normal = if flip { phys_normal } else { -phys_normal };
        return Some(Contact {
            a: if flip { ri } else { ci },
            b: if flip { ci } else { ri },
            normal,
            depth,
            contact_point,
        });
    }

    let dist = dist_sq.sqrt();
    let local_normal = Vec2::new(dx / dist, dy / dist);
    let depth = radius - dist;

    // Transform to world space — points from rect surface toward circle
    let phys_normal = Vec2::new(
        local_normal.x * c - local_normal.y * s,
        local_normal.x * s + local_normal.y * c,
    );
    // Contact point: on circle surface toward rect
    let contact_point = circle.pos - phys_normal * (radius - depth * 0.5);
    // Flip for Contact convention (same logic as above)
    let normal = if flip { phys_normal } else { -phys_normal };

    Some(Contact {
        a: if flip { ri } else { ci },
        b: if flip { ci } else { ri },
        normal,
        depth,
        contact_point,
    })
}

// ---------------------------------------------------------------------------
// SAT helpers for OBB vs OBB
// ---------------------------------------------------------------------------
fn get_rect_corners(body: &Body, hw: f32, hh: f32) -> [Vec2; 4] {
    let c = body.angle.cos();
    let s = body.angle.sin();
    let ax = Vec2::new(c, s);
    let ay = Vec2::new(-s, c);
    [
        body.pos + ax * hw + ay * hh,
        body.pos - ax * hw + ay * hh,
        body.pos - ax * hw - ay * hh,
        body.pos + ax * hw - ay * hh,
    ]
}

fn project_corners(corners: &[Vec2; 4], axis: Vec2) -> (f32, f32) {
    let mut min_p = f32::MAX;
    let mut max_p = f32::MIN;
    for &corner in corners {
        let p = corner.dot(axis);
        if p < min_p { min_p = p; }
        if p > max_p { max_p = p; }
    }
    (min_p, max_p)
}

fn point_in_obb(p: Vec2, body: &Body, hw: f32, hh: f32) -> bool {
    let d = p - body.pos;
    let c = body.angle.cos();
    let s = body.angle.sin();
    let lx = d.x * c + d.y * s;
    let ly = -d.x * s + d.y * c;
    lx.abs() <= hw && ly.abs() <= hh
}

fn collide_rect_rect(
    a: &Body, ahw: f32, ahh: f32,
    b: &Body, bhw: f32, bhh: f32,
    ia: usize, ib: usize,
) -> Option<Contact> {
    let ca = a.angle.cos();
    let sa = a.angle.sin();
    let cb = b.angle.cos();
    let sb = b.angle.sin();

    let axes = [
        Vec2::new(ca, sa),
        Vec2::new(-sa, ca),
        Vec2::new(cb, sb),
        Vec2::new(-sb, cb),
    ];

    let corners_a = get_rect_corners(a, ahw, ahh);
    let corners_b = get_rect_corners(b, bhw, bhh);

    let mut min_depth = f32::MAX;
    let mut best_axis = Vec2::default();

    for &axis in &axes {
        let (a_min, a_max) = project_corners(&corners_a, axis);
        let (b_min, b_max) = project_corners(&corners_b, axis);

        let overlap = (a_max.min(b_max)) - (a_min.max(b_min));
        if overlap <= 0.0 {
            return None; // Separating axis found
        }
        if overlap < min_depth {
            min_depth = overlap;
            best_axis = axis;
        }
    }

    // Ensure normal points from A to B
    let ab = b.pos - a.pos;
    let normal = if ab.dot(best_axis) < 0.0 { -best_axis } else { best_axis };

    // Contact point: average of all corners that penetrate into the other body.
    // This gives a proper contact surface for face-face contacts (e.g. box on floor).
    let mut cp = Vec2::default();
    let mut cp_count = 0;

    for &c in &corners_a {
        if point_in_obb(c, b, bhw, bhh) {
            cp = cp + c;
            cp_count += 1;
        }
    }
    for &c in &corners_b {
        if point_in_obb(c, a, ahw, ahh) {
            cp = cp + c;
            cp_count += 1;
        }
    }

    let contact_point = if cp_count > 0 {
        cp * (1.0 / cp_count as f32)
    } else {
        // Fallback: midpoint (rare edge-edge case)
        Vec2::new(
            (a.pos.x + b.pos.x) * 0.5,
            (a.pos.y + b.pos.y) * 0.5,
        )
    };

    Some(Contact {
        a: ia,
        b: ib,
        normal,
        depth: min_depth,
        contact_point,
    })
}

// ---------------------------------------------------------------------------
// Narrow-phase dispatch
// ---------------------------------------------------------------------------
fn collide(bodies: &[Body], i: usize, j: usize) -> Option<Contact> {
    let a = &bodies[i];
    let b = &bodies[j];
    match (a.shape, b.shape) {
        (Shape::Circle { radius: ra }, Shape::Circle { radius: rb }) => {
            collide_circle_circle(a, ra, b, rb, i, j)
        }
        (Shape::Circle { radius }, Shape::Rect { half_w, half_h }) => {
            collide_circle_rect(a, radius, b, half_w, half_h, i, j, false)
        }
        (Shape::Rect { half_w, half_h }, Shape::Circle { radius }) => {
            collide_circle_rect(b, radius, a, half_w, half_h, j, i, true)
        }
        (Shape::Rect { half_w: aw, half_h: ah }, Shape::Rect { half_w: bw, half_h: bh }) => {
            collide_rect_rect(a, aw, ah, b, bw, bh, i, j)
        }
    }
}

// ---------------------------------------------------------------------------
// Impulse resolution with friction
// ---------------------------------------------------------------------------
fn resolve(bodies: &mut [Body], contact: &Contact, restitution: f32) {
    let Contact { a, b, normal, depth, contact_point } = *contact;

    let inv_mass_sum = bodies[a].inv_mass + bodies[b].inv_mass;
    if inv_mass_sum <= 0.0 {
        return;
    }

    // Positional correction (Baumgarte stabilization)
    let slop = 0.01;
    let percent = 0.4;
    let correction = normal * (((depth - slop).max(0.0)) / inv_mass_sum * percent);
    bodies[a].pos = bodies[a].pos - correction * bodies[a].inv_mass;
    bodies[b].pos = bodies[b].pos + correction * bodies[b].inv_mass;

    // Relative velocity at contact point
    let ra = contact_point - bodies[a].pos;
    let rb = contact_point - bodies[b].pos;
    let vel_a = bodies[a].vel + Vec2::new(-bodies[a].angular_vel * ra.y, bodies[a].angular_vel * ra.x);
    let vel_b = bodies[b].vel + Vec2::new(-bodies[b].angular_vel * rb.y, bodies[b].angular_vel * rb.x);
    let rel_vel = vel_b - vel_a;

    let contact_vel = rel_vel.dot(normal);
    if contact_vel > 0.0 {
        return; // Bodies are separating
    }

    let ra_cross_n = ra.cross_scalar(normal);
    let rb_cross_n = rb.cross_scalar(normal);
    let denom = inv_mass_sum
        + ra_cross_n * ra_cross_n * bodies[a].inv_inertia
        + rb_cross_n * rb_cross_n * bodies[b].inv_inertia;

    let e = restitution;
    let j = -(1.0 + e) * contact_vel / denom;

    // Apply normal impulse
    let impulse = normal * j;
    bodies[a].vel = bodies[a].vel - impulse * bodies[a].inv_mass;
    bodies[b].vel = bodies[b].vel + impulse * bodies[b].inv_mass;
    bodies[a].angular_vel -= ra.cross_scalar(impulse) * bodies[a].inv_inertia;
    bodies[b].angular_vel += rb.cross_scalar(impulse) * bodies[b].inv_inertia;

    // Friction impulse (Coulomb model)
    // Re-compute relative velocity after normal impulse
    let vel_a2 = bodies[a].vel + Vec2::new(-bodies[a].angular_vel * ra.y, bodies[a].angular_vel * ra.x);
    let vel_b2 = bodies[b].vel + Vec2::new(-bodies[b].angular_vel * rb.y, bodies[b].angular_vel * rb.x);
    let rel_vel2 = vel_b2 - vel_a2;

    let tangent = rel_vel2 - normal * rel_vel2.dot(normal);
    let tangent_len = tangent.len();
    if tangent_len < 1e-8 {
        return;
    }
    let tangent = tangent * (1.0 / tangent_len);

    let ra_cross_t = ra.cross_scalar(tangent);
    let rb_cross_t = rb.cross_scalar(tangent);
    let denom_t = inv_mass_sum
        + ra_cross_t * ra_cross_t * bodies[a].inv_inertia
        + rb_cross_t * rb_cross_t * bodies[b].inv_inertia;

    let jt = -rel_vel2.dot(tangent) / denom_t;

    let mu = (bodies[a].friction + bodies[b].friction) * 0.5;
    let friction_impulse = if jt.abs() < j * mu {
        tangent * jt
    } else {
        tangent * (-j * mu)
    };

    bodies[a].vel = bodies[a].vel - friction_impulse * bodies[a].inv_mass;
    bodies[b].vel = bodies[b].vel + friction_impulse * bodies[b].inv_mass;
    bodies[a].angular_vel -= ra.cross_scalar(friction_impulse) * bodies[a].inv_inertia;
    bodies[b].angular_vel += rb.cross_scalar(friction_impulse) * bodies[b].inv_inertia;
}

// ---------------------------------------------------------------------------
// World (WASM-exposed)
// ---------------------------------------------------------------------------
// Flat render buffer per body: [pos_x, pos_y, angle, shape_type, dim1, dim2, color_r, color_g, color_b]
const BODY_STRIDE: usize = 9;

// Color palette for spawned bodies
const PALETTE: [[f32; 3]; 8] = [
    [0.204, 0.827, 0.600], // emerald-400
    [0.235, 0.741, 0.973], // sky-400
    [0.984, 0.573, 0.235], // orange-400
    [0.655, 0.545, 0.976], // violet-400
    [0.984, 0.443, 0.522], // rose-400
    [0.251, 0.878, 0.816], // teal-400
    [0.973, 0.741, 0.231], // amber-400
    [0.910, 0.475, 0.976], // fuchsia-400
];

#[wasm_bindgen]
pub struct PhysicsWorld {
    bodies: Vec<Body>,
    render_buf: Vec<f32>,
    width: f32,
    height: f32,
    gravity: f32,
    color_idx: usize,
    /// Damping applied every frame: velocity *= (1 - linear_damping)
    linear_damping: f32,
    angular_damping: f32,
    restitution: f32,
    dragged_body: Option<usize>,
}

#[wasm_bindgen]
impl PhysicsWorld {
    #[wasm_bindgen(constructor)]
    pub fn new(width: f32, height: f32) -> Self {
        let mut world = Self {
            bodies: Vec::new(),
            render_buf: Vec::new(),
            width,
            height,
            gravity: 600.0,
            color_idx: 0,
            linear_damping: 0.5,
            angular_damping: 2.0,
            restitution: 0.6,
            dragged_body: None,
        };

        // Floor
        world.add_static_rect(width * 0.5, height - 10.0, width, 20.0);
        // Left wall
        world.add_static_rect(-10.0, height * 0.5, 20.0, height);
        // Right wall
        world.add_static_rect(width + 10.0, height * 0.5, 20.0, height);

        world
    }

    fn add_static_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let mut body = Body::new(x, y, Shape::Rect { half_w: w * 0.5, half_h: h * 0.5 }, 0.0, 0.5, 0.6);
        body.is_static = true;
        self.bodies.push(body);
    }

    fn next_color(&mut self) -> [f32; 3] {
        let c = PALETTE[self.color_idx % PALETTE.len()];
        self.color_idx += 1;
        c
    }

    pub fn spawn_box(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let area = w * h;
        let density = 0.001;
        let mass = area * density;
        let _ = self.next_color();
        self.bodies.push(Body::new(
            x, y,
            Shape::Rect { half_w: w * 0.5, half_h: h * 0.5 },
            mass.max(0.1),
            0.5,
            0.5,
        ));
    }

    pub fn spawn_circle(&mut self, x: f32, y: f32, radius: f32) {
        let area = core::f32::consts::PI * radius * radius;
        let density = 0.001;
        let mass = area * density;
        let _ = self.next_color();
        self.bodies.push(Body::new(
            x, y,
            Shape::Circle { radius },
            mass.max(0.1),
            0.7,
            0.4,
        ));
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn set_gravity(&mut self, g: f32) {
        self.gravity = g;
    }

    pub fn set_restitution(&mut self, r: f32) {
        self.restitution = r.clamp(0.0, 1.0);
    }

    pub fn clear_dynamic(&mut self) {
        self.bodies.retain(|b| b.is_static);
        self.color_idx = 0;
    }

    // -----------------------------------------------------------------------
    // Drag (spring) interaction
    // -----------------------------------------------------------------------
    pub fn start_drag(&mut self, px: f32, py: f32) {
        let pt = Vec2::new(px, py);
        let mut best: Option<(usize, f32)> = None;

        for (i, body) in self.bodies.iter().enumerate() {
            if body.is_static { continue; }
            let d = (body.pos - pt).len_sq();
            let hit = match body.shape {
                Shape::Circle { radius } => d < radius * radius * 1.5,
                Shape::Rect { half_w, half_h } => {
                    let r = half_w.max(half_h);
                    d < r * r * 2.0
                }
            };
            if hit {
                if best.is_none() || d < best.unwrap().1 {
                    best = Some((i, d));
                }
            }
        }

        if let Some((idx, _)) = best {
            self.dragged_body = Some(idx);
            self.bodies[idx].drag_target = Some(pt);
        }
    }

    pub fn move_drag(&mut self, px: f32, py: f32) {
        if let Some(idx) = self.dragged_body {
            if idx < self.bodies.len() {
                self.bodies[idx].drag_target = Some(Vec2::new(px, py));
            }
        }
    }

    pub fn end_drag(&mut self) {
        if let Some(idx) = self.dragged_body {
            if idx < self.bodies.len() {
                self.bodies[idx].drag_target = None;
            }
        }
        self.dragged_body = None;
    }

    // -----------------------------------------------------------------------
    // Step
    // -----------------------------------------------------------------------
    pub fn step(&mut self, dt: f32) {
        let sub_steps = 4;
        let sub_dt = dt / sub_steps as f32;

        for _ in 0..sub_steps {
            self.sub_step(sub_dt);
        }
    }

    fn sub_step(&mut self, dt: f32) {
        let gravity = self.gravity;
        let linear_damping = self.linear_damping;
        let angular_damping = self.angular_damping;

        // --- Apply forces (gravity + drag spring) ---
        for body in self.bodies.iter_mut() {
            if body.is_static { continue; }

            // Gravity
            body.vel.y += gravity * dt;

            // Spring drag force toward target
            if let Some(target) = body.drag_target {
                let diff = target - body.pos;
                let spring_k = 80.0;
                let damping_k = 8.0;
                body.vel = body.vel + diff * (spring_k * dt) - body.vel * (damping_k * dt);
                // Reduce angular velocity when dragged
                body.angular_vel *= 0.9;
            }

            // Damping (friction with air) — time-dependent so it doesn't
            // scale with sub-step count.  exp(-rate * dt) is exact for
            // exponential decay; with rate=0.5 → ~40% loss per second.
            let lin_damp = (-linear_damping * dt).exp();
            let ang_damp = (-angular_damping * dt).exp();
            body.vel = body.vel * lin_damp;
            body.angular_vel *= ang_damp;
        }

        // --- Integration ---
        for body in self.bodies.iter_mut() {
            if body.is_static { continue; }
            body.pos = body.pos + body.vel * dt;
            body.angle += body.angular_vel * dt;
        }

        // --- Broad-phase (AABB) + narrow-phase ---
        let n = self.bodies.len();
        let mut contacts: Vec<Contact> = Vec::new();

        for i in 0..n {
            let (ax0, ay0, ax1, ay1) = self.bodies[i].aabb();
            for j in (i + 1)..n {
                if self.bodies[i].is_static && self.bodies[j].is_static {
                    continue;
                }
                let (bx0, by0, bx1, by1) = self.bodies[j].aabb();
                // AABB overlap test
                if ax1 < bx0 || bx1 < ax0 || ay1 < by0 || by1 < ay0 {
                    continue;
                }
                if let Some(c) = collide(&self.bodies, i, j) {
                    contacts.push(c);
                }
            }
        }

        // --- Resolve ---
        // Multiple iterations for stability
        let restitution = self.restitution;
        for _ in 0..6 {
            for contact in &contacts {
                resolve(&mut self.bodies, contact, restitution);
            }
        }

        // --- Clamp bodies that escape ---
        let w = self.width;
        let h = self.height;
        for body in self.bodies.iter_mut() {
            if body.is_static { continue; }
            if body.pos.y > h + 200.0 || body.pos.x < -200.0 || body.pos.x > w + 200.0 {
                // Teleport back into view
                body.pos = Vec2::new(w * 0.5, 50.0);
                body.vel = Vec2::default();
                body.angular_vel = 0.0;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Render buffer (zero-copy shared memory)
    // -----------------------------------------------------------------------
    pub fn fill_render_buf(&mut self) {
        let n = self.bodies.len();
        self.render_buf.resize(n * BODY_STRIDE, 0.0);

        // Track dynamic body color index
        let mut dyn_idx: usize = 0;

        for (i, body) in self.bodies.iter().enumerate() {
            let off = i * BODY_STRIDE;
            self.render_buf[off] = body.pos.x;
            self.render_buf[off + 1] = body.pos.y;
            self.render_buf[off + 2] = body.angle;

            match body.shape {
                Shape::Circle { radius } => {
                    self.render_buf[off + 3] = 0.0; // 0 = circle
                    self.render_buf[off + 4] = radius;
                    self.render_buf[off + 5] = 0.0;
                }
                Shape::Rect { half_w, half_h } => {
                    self.render_buf[off + 3] = 1.0; // 1 = rect
                    self.render_buf[off + 4] = half_w;
                    self.render_buf[off + 5] = half_h;
                }
            }

            if body.is_static {
                // Static bodies: dark zinc
                self.render_buf[off + 6] = 0.24;
                self.render_buf[off + 7] = 0.24;
                self.render_buf[off + 8] = 0.27;
            } else {
                let c = PALETTE[dyn_idx % PALETTE.len()];
                self.render_buf[off + 6] = c[0];
                self.render_buf[off + 7] = c[1];
                self.render_buf[off + 8] = c[2];
                dyn_idx += 1;
            }
        }
    }

    pub fn render_ptr(&self) -> *const f32 {
        self.render_buf.as_ptr()
    }

    pub fn render_len(&self) -> usize {
        self.render_buf.len()
    }
}
