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
    paint(cx: number, cy: number, cell_type: number, radius: number): void;
    tick(): void;
    render(): void;
    clear(): void;
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
