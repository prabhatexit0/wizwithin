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

// Boid triangle geometry (pointing right along +x).
const TIP = 6;
const WING = 3;
const HALF_W = 2.5;

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
  const flockRef = useRef<InstanceType<typeof import("@boid_engine").Flock> | null>(null);

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

          // Read boid data directly from WASM linear memory – zero copy.
          // Layout per boid: [x: f32, y: f32, vx: f32, vy: f32].
          const ptr = flock.boids_ptr();
          const n = flock.boids_count();
          const data = new Float32Array(wasmMemory.buffer, ptr, n * 4);

          // Clear.
          ctx.fillStyle = "#18181b"; // zinc-900
          ctx.fillRect(0, 0, SIM_W, SIM_H);

          // Draw each boid as a small rotated triangle.
          ctx.fillStyle = "#34d399"; // emerald-400
          for (let i = 0; i < n; i++) {
            const off = i * 4;
            const x = data[off];
            const y = data[off + 1];
            const vx = data[off + 2];
            const vy = data[off + 3];
            const angle = Math.atan2(vy, vx);

            ctx.save();
            ctx.translate(x, y);
            ctx.rotate(angle);
            ctx.beginPath();
            ctx.moveTo(TIP, 0);
            ctx.lineTo(-WING, -HALF_W);
            ctx.lineTo(-WING, HALF_W);
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
      )}

      {/* Canvas */}
      <canvas
        ref={canvasRef}
        width={SIM_W}
        height={SIM_H}
        className="rounded-lg border border-zinc-700 bg-zinc-900 w-full max-w-[800px]"
      />

      {status === "running" && (
        <p className="text-zinc-500 text-xs text-center px-2">
          Drag the sliders to tweak flocking behaviour
        </p>
      )}
    </div>
  );
}
