declare module "@snake_engine" {
  export default function init(): Promise<void>;
  export function start_snake(
    canvas_id: string,
    grid_cols: number,
    grid_rows: number,
  ): void;
}
