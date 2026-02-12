declare module "@snake_engine" {
  export default function init(): Promise<void>;
  export function start_snake(
    canvas_id: string,
    grid_cols: number,
    grid_rows: number,
  ): void;
}

declare module "@sand_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class Universe {
    constructor(width: number, height: number);
    free(): void;
    width(): number;
    height(): number;
    pixels_ptr(): number;
    pixels_len(): number;
    creatures_ptr(): number;
    creatures_count(): number;
    paint(cx: number, cy: number, cell_type: number, radius: number): void;
    spawn_creature(gx: number, gy: number, species: number): void;
    tick(): void;
    render(): void;
    clear(): void;
  }
}

declare module "@boid_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class Flock {
    constructor(width: number, height: number, count: number);
    free(): void;
    boids_ptr(): number;
    boids_count(): number;
    predators_ptr(): number;
    predators_count(): number;
    food_ptr(): number;
    food_count(): number;
    set_separation(w: number): void;
    set_alignment(w: number): void;
    set_cohesion(w: number): void;
    set_count(count: number): void;
    spawn_food(x: number, y: number): void;
    clear_food(): void;
    spawn_predator(): void;
    tick(): void;
  }
}

declare module "@chip8_core" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class Cpu {
    constructor();
    free(): void;
    load_rom(rom: Uint8Array): void;
    reset(): void;
    tick_timers(): void;
    tick_cpu(): void;
    sound_active(): boolean;
    key_down(key: number): void;
    key_up(key: number): void;
    display_ptr(): number;
    display_len(): number;
    display_width(): number;
    display_height(): number;
  }
}

declare module "@synth_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class Synth {
    constructor(sample_rate: number, buffer_size: number);
    free(): void;
    set_frequency(freq: number): void;
    set_gain(gain: number): void;
    set_waveform(waveform: number): void;
    frequency(): number;
    gain(): number;
    waveform(): number;
    fill_buffer(): void;
    buffer_ptr(): number;
    buffer_len(): number;
  }
}

declare module "@raycaster_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class World {
    constructor(width: number, height: number);
    free(): void;
    pixels_ptr(): number;
    pixels_len(): number;
    set_fov(fov_len: number): void;
    set_show_minimap(show: boolean): void;
    move_forward(speed: number): void;
    move_backward(speed: number): void;
    strafe_left(speed: number): void;
    strafe_right(speed: number): void;
    rotate_left(speed: number): void;
    rotate_right(speed: number): void;
    render(time: number, trippiness: number): void;
  }
}

declare module "@evolution_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class Simulation {
    constructor(pop_size: number);
    free(): void;
    tick(steps: number): void;
    points_ptr(): number;
    points_len(): number;
    points_per_creature(): number;
    creature_count(): number;
    muscle_indices_ptr(): number;
    muscle_count(): number;
    best_idx(): number;
    generation(): number;
    best_distance(): number;
    record_distance(): number;
    gen_progress(): number;
    ground_y(): number;
    start_x(): number;
    reset(): void;
  }
}

declare module "@physics_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class PhysicsWorld {
    constructor(width: number, height: number);
    free(): void;
    spawn_box(x: number, y: number, w: number, h: number): void;
    spawn_circle(x: number, y: number, radius: number): void;
    body_count(): number;
    set_gravity(g: number): void;
    clear_dynamic(): void;
    start_drag(px: number, py: number): void;
    move_drag(px: number, py: number): void;
    end_drag(): void;
    step(dt: number): void;
    fill_render_buf(): void;
    render_ptr(): number;
    render_len(): number;
  }
}

declare module "@fractal_engine" {
  interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  export default function init(): Promise<InitOutput>;

  export class Fractal {
    constructor(width: number, height: number);
    free(): void;
    resize(new_width: number, new_height: number): void;
    pan(dx_pixels: number, dy_pixels: number): void;
    zoom(factor: number, screen_x: number, screen_y: number): void;
    set_palette(palette: number): void;
    palette(): number;
    set_max_iter(max_iter: number): void;
    max_iter(): number;
    center_x(): number;
    center_y(): number;
    scale(): number;
    render(): void;
    buffer_ptr(): number;
    buffer_len(): number;
    reset(): void;
  }
}
