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
