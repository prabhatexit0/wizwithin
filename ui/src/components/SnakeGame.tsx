import { useEffect, useRef, useState } from "react";

const GRID_COLS = 20;
const GRID_ROWS = 20;
const CELL_PX = 20;
const CANVAS_W = GRID_COLS * CELL_PX;
const CANVAS_H = GRID_ROWS * CELL_PX;

export default function SnakeGame() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">(
    "loading",
  );
  const [errorMsg, setErrorMsg] = useState("");

  useEffect(() => {
    let cancelled = false;

    async function boot() {
      try {
        const wasm = await import("@snake_engine");
        await wasm.default();
        if (cancelled) return;

        wasm.start_snake("snake-canvas", GRID_COLS, GRID_ROWS);
        setStatus("running");
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot snake engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex flex-col items-center gap-3 w-full">
      {status === "loading" && (
        <p className="text-zinc-400 text-sm animate-pulse">
          Loading WASM module...
        </p>
      )}
      {status === "error" && (
        <p className="text-red-400 text-sm">Error: {errorMsg}</p>
      )}

      <canvas
        ref={canvasRef}
        id="snake-canvas"
        width={CANVAS_W}
        height={CANVAS_H}
        className="rounded-lg border border-zinc-700 bg-zinc-900 touch-none w-full max-w-[400px] aspect-square"
      />

      {status === "running" && (
        <div className="text-zinc-500 text-xs text-center space-y-1 px-2">
          <p className="hidden sm:block">
            Arrow keys or WASD to move &middot; R to restart
          </p>
          <p className="sm:hidden">
            Swipe to move &middot; Tap to restart when game over
          </p>
        </div>
      )}
    </div>
  );
}
