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
const WOOD = 4;
const FIRE = 5;
// SMOKE = 6  (not paintable)
const PLANT = 7;
// STEAM = 8  (not paintable)
const SOIL = 9;
const SEED = 10;
// FRUIT = 11  (not paintable — spawned by mature plants)

// Spawn creature species codes (passed to spawn_creature)
const SPAWN_RABBIT = 100;
const SPAWN_FISH = 101;
const SPAWN_BIRD = 102;

type BrushType =
  | typeof SAND
  | typeof WATER
  | typeof STONE
  | typeof EMPTY
  | typeof WOOD
  | typeof FIRE
  | typeof PLANT
  | typeof SOIL
  | typeof SEED
  | typeof SPAWN_RABBIT
  | typeof SPAWN_FISH
  | typeof SPAWN_BIRD;

interface BrushOption {
  label: string;
  value: BrushType;
  colour: string; // Tailwind bg class
  group: "material" | "flora" | "creature";
}

const BRUSHES: BrushOption[] = [
  // Materials
  { label: "Sand", value: SAND, colour: "bg-yellow-400", group: "material" },
  { label: "Water", value: WATER, colour: "bg-blue-400", group: "material" },
  { label: "Stone", value: STONE, colour: "bg-zinc-400", group: "material" },
  { label: "Wood", value: WOOD, colour: "bg-amber-700", group: "material" },
  { label: "Fire", value: FIRE, colour: "bg-orange-500", group: "material" },
  { label: "Soil", value: SOIL, colour: "bg-amber-900", group: "material" },
  {
    label: "Eraser",
    value: EMPTY,
    colour: "bg-zinc-900 border border-zinc-600",
    group: "material",
  },

  // Flora
  { label: "Plant", value: PLANT, colour: "bg-green-500", group: "flora" },
  { label: "Seed", value: SEED, colour: "bg-amber-300", group: "flora" },

  // Creatures
  {
    label: "Rabbit",
    value: SPAWN_RABBIT,
    colour: "bg-pink-300",
    group: "creature",
  },
  {
    label: "Fish",
    value: SPAWN_FISH,
    colour: "bg-orange-400",
    group: "creature",
  },
  {
    label: "Bird",
    value: SPAWN_BIRD,
    colour: "bg-sky-400",
    group: "creature",
  },
];

function isCreatureBrush(b: BrushType): boolean {
  return b === SPAWN_RABBIT || b === SPAWN_FISH || b === SPAWN_BIRD;
}

export default function SandBox() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">(
    "loading",
  );
  const [errorMsg, setErrorMsg] = useState("");
  const [brush, setBrush] = useState<BrushType>(SAND);

  // Refs that the animation loop and mouse handlers close over.
  const brushRef = useRef<BrushType>(brush);
  brushRef.current = brush;

  const universeRef = useRef<InstanceType<
    typeof import("@sand_engine").Universe
  > | null>(null);
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

      const currentBrush = brushRef.current;
      if (isCreatureBrush(currentBrush)) {
        // Spawn a creature instead of painting cells.
        const species =
          currentBrush === SPAWN_RABBIT
            ? 0
            : currentBrush === SPAWN_FISH
              ? 1
              : 2;
        universeRef.current.spawn_creature(coord[0], coord[1], species);
      } else {
        universeRef.current.paint(coord[0], coord[1], currentBrush, 2);
      }
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
  // Group brushes for display
  // -----------------------------------------------------------------------
  const materialBrushes = BRUSHES.filter((b) => b.group === "material");
  const floraBrushes = BRUSHES.filter((b) => b.group === "flora");
  const creatureBrushes = BRUSHES.filter((b) => b.group === "creature");

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
      <div className="flex flex-col gap-2 items-center w-full">
        {/* Materials row */}
        <div className="flex gap-1.5 flex-wrap justify-center">
          {materialBrushes.map((b) => (
            <button
              key={b.value}
              onClick={() => setBrush(b.value)}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs transition-colors ${
                brush === b.value
                  ? "ring-2 ring-emerald-400 bg-zinc-700 text-zinc-100"
                  : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
              }`}
            >
              <span
                className={`inline-block w-2.5 h-2.5 rounded-sm ${b.colour}`}
              />
              {b.label}
            </button>
          ))}
        </div>

        {/* Flora + Creatures row */}
        <div className="flex gap-1.5 flex-wrap justify-center">
          <span className="text-zinc-500 text-xs self-center mr-1">Flora:</span>
          {floraBrushes.map((b) => (
            <button
              key={b.value}
              onClick={() => setBrush(b.value)}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs transition-colors ${
                brush === b.value
                  ? "ring-2 ring-emerald-400 bg-zinc-700 text-zinc-100"
                  : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
              }`}
            >
              <span
                className={`inline-block w-2.5 h-2.5 rounded-sm ${b.colour}`}
              />
              {b.label}
            </button>
          ))}

          <span className="text-zinc-500 text-xs self-center ml-2 mr-1">
            Spawn:
          </span>
          {creatureBrushes.map((b) => (
            <button
              key={b.value}
              onClick={() => setBrush(b.value)}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs transition-colors ${
                brush === b.value
                  ? "ring-2 ring-purple-400 bg-zinc-700 text-zinc-100"
                  : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
              }`}
            >
              <span
                className={`inline-block w-2.5 h-2.5 rounded-sm ${b.colour}`}
              />
              {b.label}
            </button>
          ))}

          <button
            onClick={() => universeRef.current?.clear()}
            className="px-2.5 py-1 rounded-lg text-xs bg-zinc-800 text-zinc-400 hover:bg-zinc-700 transition-colors ml-2"
          >
            Clear
          </button>
        </div>
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
          Click &amp; drag to paint &middot; Select a material, flora, or
          creature above
        </p>
      )}
    </div>
  );
}
