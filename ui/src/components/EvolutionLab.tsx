import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Canvas / simulation constants
// ---------------------------------------------------------------------------
const SIM_W = 800;
const SIM_H = 500;

const DEFAULT_POP = 100;
const DEFAULT_SPEED = 1;
const MIN_SPEED = 1;
const MAX_SPEED = 20;

// Per-creature stride: 4 points × 2 floats (x, y).
const PTS_PER = 4;
const PTS_STRIDE = PTS_PER * 2;

export default function EvolutionLab() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">(
    "loading",
  );
  const [errorMsg, setErrorMsg] = useState("");

  // Slider / stat state.
  const [speed, setSpeed] = useState(DEFAULT_SPEED);
  const [gen, setGen] = useState(1);
  const [bestDist, setBestDist] = useState(0);
  const [record, setRecord] = useState(0);
  const [progress, setProgress] = useState(0);

  // Refs that the animation loop closes over.
  const simRef = useRef<InstanceType<
    typeof import("@evolution_engine").Simulation
  > | null>(null);
  const speedRef = useRef(speed);
  speedRef.current = speed;

  // -----------------------------------------------------------------------
  // Boot WASM & start render loop
  // -----------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;

    async function boot() {
      try {
        const wasm = await import("@evolution_engine");
        const exports = await wasm.default();
        if (cancelled) return;

        const wasmMemory: WebAssembly.Memory = exports.memory;
        const sim = new wasm.Simulation(DEFAULT_POP);
        simRef.current = sim;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;

        // Read topology once (same for every creature).
        const muscleCount = sim.muscle_count();
        const idxPtr = sim.muscle_indices_ptr();
        const muscleIdx = new Uint32Array(
          wasmMemory.buffer,
          idxPtr,
          muscleCount * 2,
        );
        // Copy topology so it survives memory growth.
        const topology = Array.from(muscleIdx);

        const groundY = sim.ground_y();
        const startX = sim.start_x();
        const creatureN = sim.creature_count();

        setStatus("running");

        // -- Render loop ---------------------------------------------------
        function frame() {
          if (cancelled) return;

          // Advance physics (speed slider controls substeps per frame).
          sim.tick(speedRef.current);

          // Update React state for the stats panel.
          setGen(sim.generation());
          setBestDist(sim.best_distance());
          setRecord(sim.record_distance());
          setProgress(sim.gen_progress());

          // --- Read shared-memory buffers ---
          const buf = wasmMemory.buffer;
          const ptsPtr = sim.points_ptr();
          const ptsLen = sim.points_len();
          const pts = new Float32Array(buf, ptsPtr, ptsLen);

          const bestIdx = sim.best_idx();

          // --- Camera: follow the best creature's center x ---
          let bestCx = 0;
          {
            const base = bestIdx * PTS_STRIDE;
            for (let p = 0; p < PTS_PER; p++) {
              bestCx += pts[base + p * 2];
            }
            bestCx /= PTS_PER;
          }
          const cameraX = bestCx - SIM_W * 0.35;

          // --- Draw -------------------------------------------------------
          ctx.fillStyle = "#18181b";
          ctx.fillRect(0, 0, SIM_W, SIM_H);

          // Ground.
          ctx.fillStyle = "#27272a";
          ctx.fillRect(0, groundY - cameraX * 0 + 0, SIM_W, SIM_H); // below ground
          // Actually just draw a rect from groundY to bottom:
          ctx.fillStyle = "#1f1f23";
          ctx.fillRect(0, groundY, SIM_W, SIM_H - groundY);

          ctx.strokeStyle = "#3f3f46";
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(0, groundY);
          ctx.lineTo(SIM_W, groundY);
          ctx.stroke();

          // Distance markers every 100px (in world space).
          ctx.fillStyle = "#3f3f46";
          ctx.font = "10px monospace";
          ctx.textAlign = "center";
          const markerStart = Math.floor(cameraX / 100) * 100;
          for (let wx = markerStart; wx < cameraX + SIM_W + 100; wx += 100) {
            const sx = wx - cameraX;
            // Tick mark.
            ctx.beginPath();
            ctx.moveTo(sx, groundY);
            ctx.lineTo(sx, groundY + 6);
            ctx.stroke();
            const dist = Math.round(wx - startX);
            if (dist >= 0) {
              ctx.fillText(`${dist}`, sx, groundY + 16);
            }
          }

          // Start line.
          const startSx = startX - cameraX;
          if (startSx > -10 && startSx < SIM_W + 10) {
            ctx.strokeStyle = "#f87171";
            ctx.lineWidth = 1;
            ctx.setLineDash([4, 4]);
            ctx.beginPath();
            ctx.moveTo(startSx, 0);
            ctx.lineTo(startSx, groundY);
            ctx.stroke();
            ctx.setLineDash([]);
          }

          // Ghost creatures (faint grey).
          ctx.strokeStyle = "rgba(161, 161, 170, 0.15)";
          ctx.fillStyle = "rgba(161, 161, 170, 0.2)";
          ctx.lineWidth = 1;

          for (let ci = 0; ci < creatureN; ci++) {
            if (ci === bestIdx) continue;
            drawCreature(ctx, pts, ci, cameraX, topology, muscleCount);
          }

          // Best creature (green, highlighted).
          ctx.strokeStyle = "#4ade80";
          ctx.fillStyle = "#22c55e";
          ctx.lineWidth = 2;
          drawCreature(ctx, pts, bestIdx, cameraX, topology, muscleCount);

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot evolution engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      if (simRef.current) {
        simRef.current.free();
        simRef.current = null;
      }
    };
  }, []);

  // -----------------------------------------------------------------------
  // Hard reset handler
  // -----------------------------------------------------------------------
  const onReset = useCallback(() => {
    simRef.current?.reset();
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
        <span className="w-28 text-right shrink-0">{label}</span>
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          className="w-28 sm:w-36 accent-emerald-400"
        />
        <span className="w-8 tabular-nums text-zinc-500">
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

      {/* Controls & Stats */}
      {status === "running" && (
        <div className="flex flex-col items-center gap-2">
          <div className="flex flex-wrap justify-center gap-x-6 gap-y-1">
            {/* Stats */}
            <div className="flex gap-4 text-xs tabular-nums">
              <span className="text-zinc-400">
                Gen{" "}
                <span className="text-zinc-200 font-medium">{gen}</span>
              </span>
              <span className="text-zinc-400">
                Best{" "}
                <span className="text-emerald-400 font-medium">
                  {bestDist.toFixed(0)}px
                </span>
              </span>
              <span className="text-zinc-400">
                Record{" "}
                <span className="text-amber-400 font-medium">
                  {record.toFixed(0)}px
                </span>
              </span>
            </div>
          </div>

          {/* Progress bar */}
          <div className="w-full max-w-[800px] h-1.5 rounded-full bg-zinc-800 overflow-hidden">
            <div
              className="h-full bg-emerald-500/60 transition-all duration-100"
              style={{ width: `${(progress * 100).toFixed(1)}%` }}
            />
          </div>

          <div className="flex flex-wrap justify-center items-center gap-x-4 gap-y-1">
            <Slider
              label="Sim Speed"
              value={speed}
              onChange={setSpeed}
              min={MIN_SPEED}
              max={MAX_SPEED}
              step={1}
            />
            <button
              onClick={onReset}
              className="px-3 py-1.5 rounded-lg text-sm bg-red-900/40 text-red-300 hover:bg-red-900/60 transition-colors cursor-pointer"
            >
              Hard Reset
            </button>
          </div>
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
          Creatures evolve to walk right &middot; Green = current best &middot;
          Crank speed to fast-forward generations
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Draw a single creature (muscles as lines, points as dots).
// ---------------------------------------------------------------------------
function drawCreature(
  ctx: CanvasRenderingContext2D,
  pts: Float32Array,
  ci: number,
  cameraX: number,
  topology: number[],
  muscleCount: number,
) {
  const base = ci * PTS_STRIDE;

  // Muscles (lines).
  for (let mi = 0; mi < muscleCount; mi++) {
    const a = topology[mi * 2];
    const b = topology[mi * 2 + 1];
    const ax = pts[base + a * 2] - cameraX;
    const ay = pts[base + a * 2 + 1];
    const bx = pts[base + b * 2] - cameraX;
    const by = pts[base + b * 2 + 1];
    ctx.beginPath();
    ctx.moveTo(ax, ay);
    ctx.lineTo(bx, by);
    ctx.stroke();
  }

  // Points (dots).
  for (let pi = 0; pi < PTS_PER; pi++) {
    const px = pts[base + pi * 2] - cameraX;
    const py = pts[base + pi * 2 + 1];
    ctx.beginPath();
    ctx.arc(px, py, 3, 0, Math.PI * 2);
    ctx.fill();
  }
}
