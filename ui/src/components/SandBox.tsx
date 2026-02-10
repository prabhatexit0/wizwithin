import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Grid & display constants
// ---------------------------------------------------------------------------
const GRID_W = 128;
const GRID_H = 128;
const SCALE = 4; // each sim-pixel → 4×4 CSS pixels
const CANVAS_W = GRID_W * SCALE;
const CANVAS_H = GRID_H * SCALE;

// Cell types (must match Rust constants)
const EMPTY = 0;
const SAND = 1;
const WATER = 2;
const STONE = 3;

type BrushType = typeof SAND | typeof WATER | typeof STONE | typeof EMPTY;

interface BrushOption {
  label: string;
  value: BrushType;
  colour: string; // Tailwind bg class
}

const BRUSHES: BrushOption[] = [
  { label: "Sand", value: SAND, colour: "bg-yellow-400" },
  { label: "Water", value: WATER, colour: "bg-blue-400" },
  { label: "Stone", value: STONE, colour: "bg-zinc-400" },
  { label: "Eraser", value: EMPTY, colour: "bg-zinc-900 border border-zinc-600" },
];

export default function SandBox() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const [brush, setBrush] = useState<BrushType>(SAND);

  // Refs that the animation loop and mouse handlers close over.
  const brushRef = useRef<BrushType>(brush);
  brushRef.current = brush;

  const universeRef = useRef<InstanceType<typeof import("@sand_engine").Universe> | null>(null);
  const paintingRef = useRef(false);

  // Convert mouse/touch position to grid coords.
  const toGrid = useCallback(
    (clientX: number, clientY: number): [number, number] | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const scaleX = GRID_W / rect.width;
      const scaleY = GRID_H / rect.height;
      const gx = Math.floor((clientX - rect.left) * scaleX);
      const gy = Math.floor((clientY - rect.top) * scaleY);
      if (gx < 0 || gx >= GRID_W || gy < 0 || gy >= GRID_H) return null;
      return [gx, gy];
    },
    [],
  );

  const doPaint = useCallback(
    (clientX: number, clientY: number) => {
      const coord = toGrid(clientX, clientY);
      if (!coord || !universeRef.current) return;
      universeRef.current.paint(coord[0], coord[1], brushRef.current, 2);
    },
    [toGrid],
  );

  // -----------------------------------------------------------------------
  // Boot WASM & start render loop
  // -----------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;

    async function boot() {
      try {
        const wasm = await import("@sand_engine");
        const exports = await wasm.default(); // init → returns InitOutput
        if (cancelled) return;

        // `exports.memory` is the WebAssembly.Memory backing the Rust heap.
        const wasmMemory: WebAssembly.Memory = exports.memory;

        const universe = new wasm.Universe(GRID_W, GRID_H);
        universeRef.current = universe;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;

        // Off-screen canvas at native sim resolution for crisp scaling.
        const offscreen = document.createElement("canvas");
        offscreen.width = GRID_W;
        offscreen.height = GRID_H;
        const offCtx = offscreen.getContext("2d")!;

        // Disable image smoothing so the upscale is pixel-perfect.
        ctx.imageSmoothingEnabled = false;

        setStatus("running");

        // -- Render loop (requestAnimationFrame) ---------------------------
        function frame() {
          if (cancelled) return;

          universe.tick();
          universe.render(); // writes RGBA into the pixel buffer

          // Build a view directly into WASM linear memory – zero copy.
          // We re-create the view each frame because `memory.buffer` can be
          // detached/replaced if the WASM heap grows.
          const ptr = universe.pixels_ptr();
          const len = universe.pixels_len();
          const pixels = new Uint8ClampedArray(
            wasmMemory.buffer,
            ptr,
            len,
          );

          // Paint the native-resolution image onto the off-screen canvas…
          const imageData = new ImageData(pixels, GRID_W, GRID_H);
          offCtx.putImageData(imageData, 0, 0);

          // …then scale up to the display canvas.
          ctx.drawImage(offscreen, 0, 0, CANVAS_W, CANVAS_H);

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot sand engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      if (universeRef.current) {
        universeRef.current.free();
        universeRef.current = null;
      }
    };
  }, []);

  // -----------------------------------------------------------------------
  // Mouse handlers
  // -----------------------------------------------------------------------
  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      paintingRef.current = true;
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      doPaint(e.clientX, e.clientY);
    },
    [doPaint],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!paintingRef.current) return;
      doPaint(e.clientX, e.clientY);
    },
    [doPaint],
  );

  const onPointerUp = useCallback(() => {
    paintingRef.current = false;
  }, []);

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------
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

      {/* Palette */}
      <div className="flex gap-2 flex-wrap justify-center">
        {BRUSHES.map((b) => (
          <button
            key={b.value}
            onClick={() => setBrush(b.value)}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm transition-colors ${
              brush === b.value
                ? "ring-2 ring-emerald-400 bg-zinc-700 text-zinc-100"
                : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
            }`}
          >
            <span className={`inline-block w-3 h-3 rounded-sm ${b.colour}`} />
            {b.label}
          </button>
        ))}

        <button
          onClick={() => universeRef.current?.clear()}
          className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-zinc-400 hover:bg-zinc-700 transition-colors"
        >
          Clear
        </button>
      </div>

      {/* Canvas */}
      <canvas
        ref={canvasRef}
        width={CANVAS_W}
        height={CANVAS_H}
        className="rounded-lg border border-zinc-700 bg-[#1c1c24] touch-none w-full max-w-[512px] aspect-square cursor-crosshair"
        style={{ imageRendering: "pixelated" }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
      />

      {status === "running" && (
        <p className="text-zinc-500 text-xs text-center px-2">
          Click &amp; drag to paint &middot; Select a material above
        </p>
      )}
    </div>
  );
}
