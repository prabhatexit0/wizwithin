use rand::Rng;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, KeyboardEvent, TouchEvent, WebGlBuffer, WebGlProgram,
    WebGlRenderingContext as GL,
};

// ---------------------------------------------------------------------------
// Direction / Point
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

// ---------------------------------------------------------------------------
// Game state
// ---------------------------------------------------------------------------

struct GameState {
    snake: Vec<Point>,
    direction: Direction,
    next_direction: Direction,
    food: Point,
    score: u32,
    game_over: bool,
    grid_w: i32,
    grid_h: i32,
}

impl GameState {
    fn new(grid_w: i32, grid_h: i32) -> Self {
        let mid_x = grid_w / 2;
        let mid_y = grid_h / 2;
        let snake = vec![
            Point { x: mid_x, y: mid_y },
            Point { x: mid_x - 1, y: mid_y },
            Point { x: mid_x - 2, y: mid_y },
        ];
        let mut state = Self {
            snake,
            direction: Direction::Right,
            next_direction: Direction::Right,
            food: Point { x: 0, y: 0 },
            score: 0,
            game_over: false,
            grid_w,
            grid_h,
        };
        state.spawn_food();
        state
    }

    fn spawn_food(&mut self) {
        let mut rng = rand::rng();
        loop {
            let p = Point {
                x: rng.random_range(0..self.grid_w),
                y: rng.random_range(0..self.grid_h),
            };
            if !self.snake.contains(&p) {
                self.food = p;
                return;
            }
        }
    }

    fn tick(&mut self) {
        if self.game_over {
            return;
        }

        self.direction = self.next_direction;

        let head = self.snake[0];
        let new_head = match self.direction {
            Direction::Up => Point { x: head.x, y: head.y - 1 },
            Direction::Down => Point { x: head.x, y: head.y + 1 },
            Direction::Left => Point { x: head.x - 1, y: head.y },
            Direction::Right => Point { x: head.x + 1, y: head.y },
        };

        // Wall collision
        if new_head.x < 0
            || new_head.x >= self.grid_w
            || new_head.y < 0
            || new_head.y >= self.grid_h
        {
            self.game_over = true;
            return;
        }

        // Self collision (skip tail because it will move)
        if self.snake[..self.snake.len() - 1].contains(&new_head) {
            self.game_over = true;
            return;
        }

        self.snake.insert(0, new_head);

        if new_head == self.food {
            self.score += 1;
            self.spawn_food();
        } else {
            self.snake.pop();
        }
    }

    fn set_direction(&mut self, dir: Direction) {
        // Prevent 180-degree reversal
        let dominated = matches!(
            (self.direction, dir),
            (Direction::Up, Direction::Down)
                | (Direction::Down, Direction::Up)
                | (Direction::Left, Direction::Right)
                | (Direction::Right, Direction::Left)
        );
        if !dominated {
            self.next_direction = dir;
        }
    }
}

// ---------------------------------------------------------------------------
// WebGL renderer
// ---------------------------------------------------------------------------

struct Renderer {
    gl: GL,
    program: WebGlProgram,
    vertex_buffer: WebGlBuffer,
    u_color: web_sys::WebGlUniformLocation,
    u_offset: web_sys::WebGlUniformLocation,
    u_scale: web_sys::WebGlUniformLocation,
    canvas_w: f32,
    canvas_h: f32,
    cell_w: f32,
    cell_h: f32,
}

impl Renderer {
    fn new(gl: GL, grid_w: i32, grid_h: i32) -> Result<Self, JsValue> {
        // Shaders ----------------------------------------------------------
        let vert_src = r#"
            attribute vec2 a_position;
            uniform vec2 u_offset;
            uniform vec2 u_scale;
            void main() {
                gl_Position = vec4(a_position * u_scale + u_offset, 0.0, 1.0);
            }
        "#;
        let frag_src = r#"
            precision mediump float;
            uniform vec4 u_color;
            void main() {
                gl_FragColor = u_color;
            }
        "#;

        let vert = compile_shader(&gl, GL::VERTEX_SHADER, vert_src)?;
        let frag = compile_shader(&gl, GL::FRAGMENT_SHADER, frag_src)?;
        let program = link_program(&gl, &vert, &frag)?;
        gl.use_program(Some(&program));

        // Uniforms ---------------------------------------------------------
        let u_color = gl
            .get_uniform_location(&program, "u_color")
            .ok_or("u_color not found")?;
        let u_offset = gl
            .get_uniform_location(&program, "u_offset")
            .ok_or("u_offset not found")?;
        let u_scale = gl
            .get_uniform_location(&program, "u_scale")
            .ok_or("u_scale not found")?;

        // Unit-square vertex buffer ----------------------------------------
        let vertices: [f32; 12] = [
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
            0.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ];
        let vertex_buffer = gl.create_buffer().ok_or("failed to create buffer")?;
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vertex_buffer));
        unsafe {
            let view = js_sys::Float32Array::view(&vertices);
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &view, GL::STATIC_DRAW);
        }

        let a_pos = gl.get_attrib_location(&program, "a_position") as u32;
        gl.enable_vertex_attrib_array(a_pos);
        gl.vertex_attrib_pointer_with_i32(a_pos, 2, GL::FLOAT, false, 0, 0);

        let canvas: HtmlCanvasElement = gl.canvas().unwrap().dyn_into()?;
        let canvas_w = canvas.width() as f32;
        let canvas_h = canvas.height() as f32;
        let cell_w = canvas_w / grid_w as f32;
        let cell_h = canvas_h / grid_h as f32;

        Ok(Self {
            gl,
            program,
            vertex_buffer,
            u_color,
            u_offset,
            u_scale,
            canvas_w,
            canvas_h,
            cell_w,
            cell_h,
        })
    }

    /// Map a grid cell to clip-space offset + scale and draw it.
    fn draw_cell(&self, x: i32, y: i32, r: f32, g: f32, b: f32) {
        let gl = &self.gl;
        let padding = 1.0; // 1px gap between cells

        // pixel coords
        let px = x as f32 * self.cell_w + padding;
        let py = y as f32 * self.cell_h + padding;
        let pw = self.cell_w - padding * 2.0;
        let ph = self.cell_h - padding * 2.0;

        // to clip space  (-1..1)
        let cx = px / self.canvas_w * 2.0 - 1.0;
        let cy = 1.0 - (py + ph) / self.canvas_h * 2.0; // flip Y
        let sw = pw / self.canvas_w * 2.0;
        let sh = ph / self.canvas_h * 2.0;

        gl.uniform2f(Some(&self.u_offset), cx, cy);
        gl.uniform2f(Some(&self.u_scale), sw, sh);
        gl.uniform4f(Some(&self.u_color), r, g, b, 1.0);
        gl.draw_arrays(GL::TRIANGLES, 0, 6);
    }

    fn render(&self, state: &GameState) {
        let gl = &self.gl;
        gl.viewport(0, 0, self.canvas_w as i32, self.canvas_h as i32);

        // Background
        gl.clear_color(0.11, 0.11, 0.14, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT);

        gl.use_program(Some(&self.program));
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.vertex_buffer));

        let a_pos = gl.get_attrib_location(&self.program, "a_position") as u32;
        gl.enable_vertex_attrib_array(a_pos);
        gl.vertex_attrib_pointer_with_i32(a_pos, 2, GL::FLOAT, false, 0, 0);

        // Draw grid lines (subtle)
        for x in 0..state.grid_w {
            for y in 0..state.grid_h {
                self.draw_cell(x, y, 0.15, 0.15, 0.18);
            }
        }

        // Draw food
        self.draw_cell(state.food.x, state.food.y, 0.95, 0.26, 0.26);

        // Draw snake body
        for (i, seg) in state.snake.iter().enumerate() {
            if i == 0 {
                // head: bright green
                self.draw_cell(seg.x, seg.y, 0.30, 0.85, 0.40);
            } else {
                // body: slightly darker green
                self.draw_cell(seg.x, seg.y, 0.20, 0.70, 0.30);
            }
        }

        // Game-over overlay (dim the board)
        if state.game_over {
            gl.enable(GL::BLEND);
            gl.blend_func(GL::SRC_ALPHA, GL::ONE_MINUS_SRC_ALPHA);
            gl.uniform2f(Some(&self.u_offset), -1.0, -1.0);
            gl.uniform2f(Some(&self.u_scale), 2.0, 2.0);
            gl.uniform4f(Some(&self.u_color), 0.0, 0.0, 0.0, 0.55);
            gl.draw_arrays(GL::TRIANGLES, 0, 6);
            gl.disable(GL::BLEND);
        }
    }
}

// ---------------------------------------------------------------------------
// Shader helpers
// ---------------------------------------------------------------------------

fn compile_shader(gl: &GL, shader_type: u32, source: &str) -> Result<web_sys::WebGlShader, String> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or("unable to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    if gl
        .get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(gl
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| "unknown error".into()))
    }
}

fn link_program(
    gl: &GL,
    vert: &web_sys::WebGlShader,
    frag: &web_sys::WebGlShader,
) -> Result<WebGlProgram, String> {
    let program = gl.create_program().ok_or("unable to create program")?;
    gl.attach_shader(&program, vert);
    gl.attach_shader(&program, frag);
    gl.link_program(&program);
    if gl
        .get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(gl
            .get_program_info_log(&program)
            .unwrap_or_else(|| "unknown error".into()))
    }
}

// ---------------------------------------------------------------------------
// Closure helper for requestAnimationFrame
// ---------------------------------------------------------------------------

fn window() -> web_sys::Window {
    web_sys::window().expect("no global window")
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame`");
}

// ---------------------------------------------------------------------------
// Public WASM API
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub fn start_snake(canvas_id: &str, grid_cols: i32, grid_rows: i32) -> Result<(), JsValue> {
    // Grab canvas & GL context
    let document = window().document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<HtmlCanvasElement>()?;

    let gl: GL = canvas
        .get_context("webgl")?
        .ok_or("webgl not supported")?
        .dyn_into()?;

    let renderer = Renderer::new(gl, grid_cols, grid_rows)?;
    let state = GameState::new(grid_cols, grid_rows);

    let state = Rc::new(RefCell::new(state));
    let renderer = Rc::new(renderer);

    // Keyboard input -------------------------------------------------------
    {
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            let mut s = state.borrow_mut();
            match e.key().as_str() {
                "ArrowUp" | "w" | "W" => s.set_direction(Direction::Up),
                "ArrowDown" | "s" | "S" => s.set_direction(Direction::Down),
                "ArrowLeft" | "a" | "A" => s.set_direction(Direction::Left),
                "ArrowRight" | "d" | "D" => s.set_direction(Direction::Right),
                "r" | "R" => {
                    *s = GameState::new(s.grid_w, s.grid_h);
                }
                _ => {}
            }
            e.prevent_default();
        });
        window()
            .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
        closure.forget(); // leak – lives for the page lifetime
    }

    // Touch / swipe input (mobile) -----------------------------------------
    {
        let touch_start: Rc<RefCell<Option<(f64, f64)>>> = Rc::new(RefCell::new(None));

        // touchstart – record the starting point
        {
            let touch_start = Rc::clone(&touch_start);
            let closure =
                Closure::<dyn FnMut(TouchEvent)>::new(move |e: TouchEvent| {
                    if let Some(touch) = e.touches().get(0) {
                        *touch_start.borrow_mut() =
                            Some((touch.client_x() as f64, touch.client_y() as f64));
                    }
                });
            canvas
                .add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref())?;
            closure.forget();
        }

        // touchend – compute swipe direction
        {
            let touch_start = Rc::clone(&touch_start);
            let state = Rc::clone(&state);
            let closure =
                Closure::<dyn FnMut(TouchEvent)>::new(move |e: TouchEvent| {
                    e.prevent_default(); // prevent scroll / zoom
                    let start = *touch_start.borrow();
                    if let Some((sx, sy)) = start {
                        if let Some(touch) = e.changed_touches().get(0) {
                            let dx = touch.client_x() as f64 - sx;
                            let dy = touch.client_y() as f64 - sy;
                            let min_swipe = 20.0; // minimum px to count as a swipe

                            if dx.abs() > min_swipe || dy.abs() > min_swipe {
                                let mut s = state.borrow_mut();
                                if dx.abs() > dy.abs() {
                                    // horizontal swipe
                                    if dx > 0.0 {
                                        s.set_direction(Direction::Right);
                                    } else {
                                        s.set_direction(Direction::Left);
                                    }
                                } else {
                                    // vertical swipe
                                    if dy > 0.0 {
                                        s.set_direction(Direction::Down);
                                    } else {
                                        s.set_direction(Direction::Up);
                                    }
                                }
                            } else {
                                // Tap (no significant swipe) – reset if game over
                                let s = state.borrow();
                                if s.game_over {
                                    drop(s);
                                    let mut s = state.borrow_mut();
                                    *s = GameState::new(s.grid_w, s.grid_h);
                                }
                            }
                        }
                    }
                    *touch_start.borrow_mut() = None;
                });
            canvas
                .add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref())?;
            closure.forget();
        }

        // touchmove – prevent scrolling while swiping on the canvas
        {
            let closure =
                Closure::<dyn FnMut(TouchEvent)>::new(move |e: TouchEvent| {
                    e.prevent_default();
                });
            canvas
                .add_event_listener_with_callback("touchmove", closure.as_ref().unchecked_ref())?;
            closure.forget();
        }
    }

    // Game loop (tick at ~8 fps, render at display rate) --------------------
    let tick_interval_ms = 120.0; // ms between game ticks
    let last_tick: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = Rc::clone(&f);

    let state2 = Rc::clone(&state);
    let renderer2 = Rc::clone(&renderer);
    let last_tick2 = Rc::clone(&last_tick);

    *g.borrow_mut() = Some(Closure::new(move || {
        let now = js_sys::Date::now();
        let mut lt = last_tick2.borrow_mut();
        if now - *lt >= tick_interval_ms {
            state2.borrow_mut().tick();
            *lt = now;
        }
        renderer2.render(&state2.borrow());

        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());

    Ok(())
}

/// Returns the current score for the running game.
/// (Useful for React to poll & display outside the canvas.)
#[wasm_bindgen]
pub fn get_score() -> u32 {
    // For the prototype we return 0 – wiring a shared score requires
    // storing the Rc<RefCell<GameState>> in a static. We'll do that
    // in a follow-up iteration.
    0
}
