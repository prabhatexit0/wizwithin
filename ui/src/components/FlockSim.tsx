import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Canvas / simulation constants
// ---------------------------------------------------------------------------
const SIM_W = 800;
const SIM_H = 500;

const DEFAULT_COUNT = 300;
const MIN_COUNT = 20;
const MAX_COUNT = 1000;

const DEFAULT_SEP = 1.5;
const DEFAULT_ALI = 1.0;
const DEFAULT_COH = 1.0;

// Per-boid stride in the Float32Array (x, y, vx, vy, eat_timer).
const BOID_STRIDE = 5;
// Per-food stride (x, y).
const FOOD_STRIDE = 2;

// Boid triangle geometry (pointing right along +x).
const TIP = 6;
const WING = 3;
const HALF_W = 2.5;

// Predator triangle geometry (larger).
const PRED_TIP = 10;
const PRED_WING = 5;
const PRED_HALF_W = 4;

// Food dot radius.
const FOOD_R = 3;

export default function FlockSim() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">(
    "loading",
  );
  const [errorMsg, setErrorMsg] = useState("");

  // Slider state.
  const [separation, setSeparation] = useState(DEFAULT_SEP);
  const [alignment, setAlignment] = useState(DEFAULT_ALI);
  const [cohesion, setCohesion] = useState(DEFAULT_COH);
  const [count, setCount] = useState(DEFAULT_COUNT);

  // Refs that the animation loop closes over.
  const flockRef = useRef<InstanceType<
    typeof import("@boid_engine").Flock
  > | null>(null);

  // Sync slider changes into WASM each frame via refs.
  const sepRef = useRef(separation);
  sepRef.current = separation;
  const aliRef = useRef(alignment);
  aliRef.current = alignment;
  const cohRef = useRef(cohesion);
  cohRef.current = cohesion;
  const countRef = useRef(count);
  countRef.current = count;

  // -----------------------------------------------------------------------
  // Boot WASM & start render loop
  // -----------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;

    async function boot() {
      try {
        const wasm = await import("@boid_engine");
        const exports = await wasm.default();
        if (cancelled) return;

        const wasmMemory: WebAssembly.Memory = exports.memory;

        const flock = new wasm.Flock(SIM_W, SIM_H, DEFAULT_COUNT);
        flockRef.current = flock;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;

        setStatus("running");

        // -- Render loop ---------------------------------------------------
        function frame() {
          if (cancelled) return;

          // Push slider values into WASM.
          flock.set_separation(sepRef.current);
          flock.set_alignment(aliRef.current);
          flock.set_cohesion(cohRef.current);
          flock.set_count(countRef.current);

          flock.tick();

          // --- Read shared-memory buffers from WASM ----------------------
          const buf = wasmMemory.buffer;

          const boidPtr = flock.boids_ptr();
          const boidN = flock.boids_count();
          const boids = new Float32Array(buf, boidPtr, boidN * BOID_STRIDE);

          const predPtr = flock.predators_ptr();
          const predN = flock.predators_count();
          const preds = new Float32Array(buf, predPtr, predN * BOID_STRIDE);

          const foodPtr = flock.food_ptr();
          const foodN = flock.food_count();
          const foods = new Float32Array(buf, foodPtr, foodN * FOOD_STRIDE);

          // --- Draw ------------------------------------------------------

          // Clear.
          ctx.fillStyle = "#18181b"; // zinc-900
          ctx.fillRect(0, 0, SIM_W, SIM_H);

          // 1. Food – small green circles.
          ctx.fillStyle = "#4ade80"; // green-400
          for (let i = 0; i < foodN; i++) {
            const off = i * FOOD_STRIDE;
            ctx.beginPath();
            ctx.arc(foods[off], foods[off + 1], FOOD_R, 0, Math.PI * 2);
            ctx.fill();
          }

          // 2. Boids – emerald triangles with eat-pop scale.
          ctx.fillStyle = "#34d399"; // emerald-400
          for (let i = 0; i < boidN; i++) {
            const off = i * BOID_STRIDE;
            const x = boids[off];
            const y = boids[off + 1];
            const vx = boids[off + 2];
            const vy = boids[off + 3];
            const eatTimer = boids[off + 4];
            const angle = Math.atan2(vy, vx);
            const scale = eatTimer > 0 ? 1.0 + eatTimer * 0.5 : 1.0;

            ctx.save();
            ctx.translate(x, y);
            ctx.rotate(angle);
            if (scale !== 1.0) ctx.scale(scale, scale);
            ctx.beginPath();
            ctx.moveTo(TIP, 0);
            ctx.lineTo(-WING, -HALF_W);
            ctx.lineTo(-WING, HALF_W);
            ctx.closePath();
            ctx.fill();
            ctx.restore();
          }

          // 3. Predators – larger red triangles.
          ctx.fillStyle = "#ef4444"; // red-500
          for (let i = 0; i < predN; i++) {
            const off = i * BOID_STRIDE;
            const x = preds[off];
            const y = preds[off + 1];
            const vx = preds[off + 2];
            const vy = preds[off + 3];
            const angle = Math.atan2(vy, vx);

            ctx.save();
            ctx.translate(x, y);
            ctx.rotate(angle);
            ctx.beginPath();
            ctx.moveTo(PRED_TIP, 0);
            ctx.lineTo(-PRED_WING, -PRED_HALF_W);
            ctx.lineTo(-PRED_WING, PRED_HALF_W);
            ctx.closePath();
            ctx.fill();
            ctx.restore();
          }

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot boid engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      if (flockRef.current) {
        flockRef.current.free();
        flockRef.current = null;
      }
    };
  }, []);

  // -----------------------------------------------------------------------
  // Click handler – left-click spawns a cluster of food
  // -----------------------------------------------------------------------
  const onCanvasClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      const canvas = canvasRef.current;
      const flock = flockRef.current;
      if (!canvas || !flock) return;

      const rect = canvas.getBoundingClientRect();
      const sx = SIM_W / rect.width;
      const sy = SIM_H / rect.height;
      const cx = (e.clientX - rect.left) * sx;
      const cy = (e.clientY - rect.top) * sy;

      // Drop a small cluster of 5 food items.
      for (let i = 0; i < 5; i++) {
        flock.spawn_food(
          cx + (Math.random() - 0.5) * 20,
          cy + (Math.random() - 0.5) * 20,
        );
      }
    },
    [],
  );

  // -----------------------------------------------------------------------
  // Slider helper
  // -----------------------------------------------------------------------
  const Slider = useCallback(
    ({
      label,
      value,
      onChange,
      min,
      max,
      step,
    }: {
      label: string;
      value: number;
      onChange: (v: number) => void;
      min: number;
      max: number;
      step: number;
    }) => (
      <label className="flex items-center gap-2 text-xs text-zinc-400">
        <span className="w-24 text-right shrink-0">{label}</span>
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="w-28 sm:w-36 accent-emerald-400"
        />
        <span className="w-10 tabular-nums text-zinc-500">
          {value.toFixed(step < 1 ? 1 : 0)}
        </span>
      </label>
    ),
    [],
  );

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------
  return (
    <div className="flex flex-col items-center gap-3 w-full">
      {status === "loading" && (
        <p className="text-zinc-400 text-sm animate-pulse">
          Loading WASM module&hellip;
        </p>
      )}
      {status === "error" && (
        <p className="text-red-400 text-sm">Error: {errorMsg}</p>
      )}

      {/* Controls */}
      {status === "running" && (
        <div className="flex flex-col items-center gap-2">
          <div className="flex flex-wrap justify-center gap-x-4 gap-y-1">
            <Slider
              label="Separation"
              value={separation}
              onChange={setSeparation}
              min={0}
              max={4}
              step={0.1}
            />
            <Slider
              label="Alignment"
              value={alignment}
              onChange={setAlignment}
              min={0}
              max={4}
              step={0.1}
            />
            <Slider
              label="Cohesion"
              value={cohesion}
              onChange={setCohesion}
              min={0}
              max={4}
              step={0.1}
            />
            <Slider
              label="Count"
              value={count}
              onChange={setCount}
              min={MIN_COUNT}
              max={MAX_COUNT}
              step={10}
            />
          </div>

          {/* Action buttons */}
          <div className="flex gap-2">
            <button
              onClick={() => flockRef.current?.spawn_predator()}
              className="px-3 py-1.5 rounded-lg text-sm bg-red-900/40 text-red-300 hover:bg-red-900/60 transition-colors"
            >
              Add Predator
            </button>
            <button
              onClick={() => flockRef.current?.clear_food()}
              className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-zinc-400 hover:bg-zinc-700 transition-colors"
            >
              Clear Food
            </button>
          </div>
        </div>
      )}

      {/* Canvas */}
      <canvas
        ref={canvasRef}
        width={SIM_W}
        height={SIM_H}
        className="rounded-lg border border-zinc-700 bg-zinc-900 w-full max-w-[800px] cursor-crosshair"
        onClick={onCanvasClick}
      />

      {status === "running" && (
        <p className="text-zinc-500 text-xs text-center px-2">
          Click canvas to drop food &middot; Drag sliders to tweak flocking
        </p>
      )}
    </div>
  );
}
