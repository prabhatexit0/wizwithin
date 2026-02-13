import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const SIM_W = 800;
const SIM_H = 500;

// Per-body stride in the render buffer:
// [pos_x, pos_y, angle, shape_type, dim1, dim2, r, g, b]
const BODY_STRIDE = 9;

const SHAPE_SIZES = [
  { label: "Small", boxW: 25, boxH: 25, radius: 15 },
  { label: "Medium", boxW: 45, boxH: 45, radius: 25 },
  { label: "Large", boxW: 70, boxH: 70, radius: 38 },
];

type ShapeTool = "box" | "circle";

export default function PhysicsLab() {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">(
    "loading",
  );
  const [errorMsg, setErrorMsg] = useState("");

  // UI state (safe to use useState — these change infrequently on button press)
  const [shapeTool, setShapeTool] = useState<ShapeTool>("box");
  const [sizeIdx, setSizeIdx] = useState(1); // medium
  const [gravity, setGravity] = useState(600);
  const [bounce, setBounce] = useState(0.6);

  // Refs for the render loop (no re-renders!)
  const worldRef = useRef<InstanceType<
    typeof import("@physics_engine").PhysicsWorld
  > | null>(null);
  const shapeToolRef = useRef(shapeTool);
  shapeToolRef.current = shapeTool;
  const sizeIdxRef = useRef(sizeIdx);
  sizeIdxRef.current = sizeIdx;
  const gravityRef = useRef(gravity);
  gravityRef.current = gravity;
  const bounceRef = useRef(bounce);
  bounceRef.current = bounce;

  // -------------------------------------------------------------------------
  // Boot WASM & start render loop
  // -------------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;

    async function boot() {
      try {
        const wasm = await import("@physics_engine");
        const exports = await wasm.default();
        if (cancelled) return;

        const wasmMemory: WebAssembly.Memory = exports.memory;
        const world = new wasm.PhysicsWorld(SIM_W, SIM_H);
        worldRef.current = world;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;

        // ---- Retina / High-DPI handling ----
        function syncCanvasSize() {
          const container = containerRef.current;
          if (!container) return;
          const rect = container.getBoundingClientRect();
          const dpr = window.devicePixelRatio || 1;
          const displayW = rect.width;
          const displayH = (displayW / SIM_W) * SIM_H;

          canvas.style.width = `${displayW}px`;
          canvas.style.height = `${displayH}px`;
          canvas.width = Math.round(displayW * dpr);
          canvas.height = Math.round(displayH * dpr);

          ctx.setTransform(1, 0, 0, 1, 0, 0);
          ctx.scale(
            (displayW * dpr) / SIM_W,
            (displayH * dpr) / SIM_H,
          );
        }

        syncCanvasSize();

        // ---- ResizeObserver for responsive sizing ----
        const ro = new ResizeObserver(() => {
          if (!cancelled) syncCanvasSize();
        });
        ro.observe(containerRef.current!);

        setStatus("running");

        // ---- Spawn a few starter bodies ----
        world.spawn_box(200, 100, 45, 45);
        world.spawn_box(350, 80, 45, 45);
        world.spawn_circle(500, 120, 25);
        world.spawn_circle(300, 50, 25);
        world.spawn_box(600, 60, 70, 35);

        let lastTime = performance.now();

        // ---- Render loop ----
        function frame(now: number) {
          if (cancelled) return;

          const dt = Math.min((now - lastTime) / 1000, 0.033); // cap at ~30fps min
          lastTime = now;

          world.set_gravity(gravityRef.current);
          world.set_restitution(bounceRef.current);
          world.step(dt);
          world.fill_render_buf();

          // Read render buffer from WASM shared memory
          const ptr = world.render_ptr();
          const len = world.render_len();
          const buf = new Float32Array(wasmMemory.buffer, ptr, len);
          const bodyCount = world.body_count();

          // ---- Draw ----
          ctx.save();
          // Clear
          ctx.fillStyle = "#18181b";
          ctx.fillRect(0, 0, SIM_W, SIM_H);

          // Ground line
          ctx.strokeStyle = "#3f3f46";
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(0, SIM_H - 20);
          ctx.lineTo(SIM_W, SIM_H - 20);
          ctx.stroke();

          // Bodies
          for (let i = 0; i < bodyCount; i++) {
            const off = i * BODY_STRIDE;
            const px = buf[off];
            const py = buf[off + 1];
            const angle = buf[off + 2];
            const shapeType = buf[off + 3];
            const dim1 = buf[off + 4];
            const dim2 = buf[off + 5];
            const r = Math.round(buf[off + 6] * 255);
            const g = Math.round(buf[off + 7] * 255);
            const b = Math.round(buf[off + 8] * 255);

            ctx.save();
            ctx.translate(px, py);
            ctx.rotate(angle);

            ctx.fillStyle = `rgb(${r},${g},${b})`;

            if (shapeType < 0.5) {
              // Circle
              ctx.beginPath();
              ctx.arc(0, 0, dim1, 0, Math.PI * 2);
              ctx.fill();
              // Orientation line
              ctx.strokeStyle = `rgba(${r},${g},${b},0.5)`;
              ctx.lineWidth = 1.5;
              ctx.beginPath();
              ctx.moveTo(0, 0);
              ctx.lineTo(dim1 * 0.8, 0);
              ctx.stroke();
            } else {
              // Rect
              ctx.fillRect(-dim1, -dim2, dim1 * 2, dim2 * 2);
              // Subtle border
              ctx.strokeStyle = "rgba(255,255,255,0.08)";
              ctx.lineWidth = 1;
              ctx.strokeRect(-dim1, -dim2, dim1 * 2, dim2 * 2);
            }

            ctx.restore();
          }

          ctx.restore();
          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);

        // Cleanup additions for ResizeObserver
        const cleanup = () => {
          ro.disconnect();
        };
        // Store for outer cleanup
        (canvas as any).__roCleanup = cleanup;
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot physics engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      const canvas = canvasRef.current;
      if (canvas && (canvas as any).__roCleanup) {
        (canvas as any).__roCleanup();
      }
      if (worldRef.current) {
        worldRef.current.free();
        worldRef.current = null;
      }
    };
  }, []);

  // -------------------------------------------------------------------------
  // Pointer-to-sim coordinate conversion
  // -------------------------------------------------------------------------
  const toSim = useCallback(
    (clientX: number, clientY: number): [number, number] | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const sx = SIM_W / rect.width;
      const sy = SIM_H / rect.height;
      return [(clientX - rect.left) * sx, (clientY - rect.top) * sy];
    },
    [],
  );

  // -------------------------------------------------------------------------
  // Pointer events — drag existing bodies or spawn new ones
  // -------------------------------------------------------------------------
  const isDragging = useRef(false);
  const didDrag = useRef(false);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const world = worldRef.current;
      const pt = toSim(e.clientX, e.clientY);
      if (!world || !pt) return;

      (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);
      isDragging.current = true;
      didDrag.current = false;

      // Try to grab an existing body
      world.start_drag(pt[0], pt[1]);
    },
    [toSim],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!isDragging.current) return;
      didDrag.current = true;
      const world = worldRef.current;
      const pt = toSim(e.clientX, e.clientY);
      if (!world || !pt) return;
      world.move_drag(pt[0], pt[1]);
    },
    [toSim],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const world = worldRef.current;
      if (!world) return;

      world.end_drag();

      // If it was a tap (no drag), spawn a shape
      if (!didDrag.current) {
        const pt = toSim(e.clientX, e.clientY);
        if (pt) {
          const size = SHAPE_SIZES[sizeIdxRef.current];
          if (shapeToolRef.current === "box") {
            world.spawn_box(pt[0], pt[1], size.boxW, size.boxH);
          } else {
            world.spawn_circle(pt[0], pt[1], size.radius);
          }
        }
      }

      isDragging.current = false;
      didDrag.current = false;
    },
    [toSim],
  );

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------
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
        <div className="flex flex-wrap justify-center gap-2">
          {/* Shape toggle */}
          <div className="flex rounded-lg overflow-hidden border border-zinc-700">
            <button
              onClick={() => setShapeTool("box")}
              className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                shapeTool === "box"
                  ? "bg-emerald-600 text-white"
                  : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
              }`}
            >
              Box
            </button>
            <button
              onClick={() => setShapeTool("circle")}
              className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                shapeTool === "circle"
                  ? "bg-emerald-600 text-white"
                  : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
              }`}
            >
              Circle
            </button>
          </div>

          {/* Size toggle */}
          <div className="flex rounded-lg overflow-hidden border border-zinc-700">
            {SHAPE_SIZES.map((s, i) => (
              <button
                key={s.label}
                onClick={() => setSizeIdx(i)}
                className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                  sizeIdx === i
                    ? "bg-sky-600 text-white"
                    : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                }`}
              >
                {s.label}
              </button>
            ))}
          </div>

          {/* Gravity slider */}
          <label className="flex items-center gap-2 text-xs text-zinc-400">
            <span className="shrink-0">Gravity</span>
            <input
              type="range"
              min={0}
              max={1500}
              step={50}
              value={gravity}
              onChange={(e) => setGravity(Number(e.target.value))}
              className="w-20 sm:w-28 accent-emerald-400"
            />
            <span className="w-10 tabular-nums text-zinc-500">{gravity}</span>
          </label>

          {/* Bounce slider */}
          <label className="flex items-center gap-2 text-xs text-zinc-400">
            <span className="shrink-0">Bounce</span>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={bounce}
              onChange={(e) => setBounce(Number(e.target.value))}
              className="w-20 sm:w-28 accent-amber-400"
            />
            <span className="w-10 tabular-nums text-zinc-500">{bounce.toFixed(2)}</span>
          </label>

          {/* Clear */}
          <button
            onClick={() => worldRef.current?.clear_dynamic()}
            className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-zinc-400 hover:bg-zinc-700 border border-zinc-700 transition-colors cursor-pointer"
          >
            Clear
          </button>
        </div>
      )}

      {/* Canvas */}
      <div ref={containerRef} className="w-full max-w-[800px]">
        <canvas
          ref={canvasRef}
          className="rounded-lg border border-zinc-700 bg-zinc-900 w-full cursor-crosshair"
          style={{ touchAction: "none" }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        />
      </div>

      {status === "running" && (
        <p className="text-zinc-500 text-xs text-center px-2">
          Tap to spawn &middot; Drag to grab &amp; throw &middot; Objects have mass, momentum &amp; friction
        </p>
      )}
    </div>
  );
}
