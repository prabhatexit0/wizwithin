import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const SIM_W = 400;
const SIM_H = 300;

// Material IDs (must match Rust constants)
const EMPTY = 0;
const WOOD = 1;
const STONE = 2;
const STEEL = 3;
const GLASS = 4;

// Bomb type IDs
const BOMB_C4 = 0;
const BOMB_THERMITE = 1;
const BOMB_DIRTY = 2;

type DrawTool = "wood" | "stone" | "steel" | "glass" | "eraser";
type BombTool = "c4" | "thermite" | "dirty";

const DRAW_TOOLS: { id: DrawTool; label: string; material: number; color: string }[] = [
  { id: "steel", label: "Steel", material: STEEL, color: "#b4c3d2" },
  { id: "wood", label: "Wood", material: WOOD, color: "#8b5a2b" },
  { id: "stone", label: "Stone", material: STONE, color: "#8c8c8c" },
  { id: "glass", label: "Glass", material: GLASS, color: "#aad7e6" },
  { id: "eraser", label: "Eraser", material: EMPTY, color: "#555" },
];

const BOMB_TOOLS: { id: BombTool; label: string; type: number; color: string; desc: string }[] = [
  { id: "c4", label: "C4", type: BOMB_C4, color: "#f97316", desc: "Kinetic shockwave" },
  { id: "thermite", label: "Thermite", type: BOMB_THERMITE, color: "#ef4444", desc: "Extreme heat" },
  { id: "dirty", label: "Dirty Bomb", type: BOMB_DIRTY, color: "#22c55e", desc: "Radiation fallout" },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function BlastLab() {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");

  // UI state
  const [drawTool, setDrawTool] = useState<DrawTool>("steel");
  const [bombTool, setBombTool] = useState<BombTool>("c4");
  const [brushSize, setBrushSize] = useState(3);
  const [bombCount, setBombCount] = useState(0);
  const [isDetonated, setIsDetonated] = useState(false);

  // HUD telemetry (updated from RAF loop)
  const [hudKinetic, setHudKinetic] = useState(0);
  const [hudTemp, setHudTemp] = useState(0);
  const [hudDestroyed, setHudDestroyed] = useState(0);

  // Refs for the render loop
  const simRef = useRef<InstanceType<typeof import("@blast_lab").BlastLabSim> | null>(null);
  const drawToolRef = useRef(drawTool);
  drawToolRef.current = drawTool;
  const bombToolRef = useRef(bombTool);
  bombToolRef.current = bombTool;
  const brushSizeRef = useRef(brushSize);
  brushSizeRef.current = brushSize;
  const isDetonatedRef = useRef(isDetonated);
  isDetonatedRef.current = isDetonated;

  // -------------------------------------------------------------------------
  // Boot WASM & animation loop
  // -------------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;

    async function boot() {
      try {
        const wasm = await import("@blast_lab");
        const exports = await wasm.default();
        if (cancelled) return;

        const wasmMemory: WebAssembly.Memory = exports.memory;
        const sim = new wasm.BlastLabSim(SIM_W, SIM_H);
        simRef.current = sim;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;

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

        const ro = new ResizeObserver(() => {
          if (!cancelled) syncCanvasSize();
        });
        ro.observe(containerRef.current!);

        setStatus("running");

        let frameCount = 0;

        function frame() {
          if (cancelled) return;

          // Run physics ticks if we've detonated
          if (isDetonatedRef.current) {
            sim.tick();
          }

          sim.render();

          // Read pixel buffer from shared memory
          const ptr = sim.pixels_ptr();
          const len = sim.pixels_len();
          const pixels = new Uint8ClampedArray(wasmMemory.buffer, ptr, len);
          const imageData = new ImageData(pixels, SIM_W, SIM_H);
          ctx.putImageData(imageData, 0, 0);

          // Update HUD every 6 frames to avoid excessive React renders
          frameCount++;
          if (frameCount % 6 === 0) {
            setHudKinetic(sim.stats_peak_kinetic());
            setHudTemp(sim.stats_peak_temp());
            setHudDestroyed(sim.stats_pixels_destroyed());
            setBombCount(sim.bomb_count());
          }

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);

        (canvas as any).__roCleanup = () => ro.disconnect();
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot blast_lab:", err);
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
      if (simRef.current) {
        simRef.current.free();
        simRef.current = null;
      }
    };
  }, []);

  // -------------------------------------------------------------------------
  // Coordinate conversion
  // -------------------------------------------------------------------------
  const toSim = useCallback(
    (clientX: number, clientY: number): [number, number] | null => {
      const canvas = canvasRef.current;
      if (!canvas) return null;
      const rect = canvas.getBoundingClientRect();
      const sx = SIM_W / rect.width;
      const sy = SIM_H / rect.height;
      return [
        Math.floor((clientX - rect.left) * sx),
        Math.floor((clientY - rect.top) * sy),
      ];
    },
    [],
  );

  // -------------------------------------------------------------------------
  // Pointer events — left-drag to draw, right-click to place bomb
  // -------------------------------------------------------------------------
  const isDrawing = useRef(false);

  const paintAt = useCallback(
    (clientX: number, clientY: number) => {
      const sim = simRef.current;
      const pt = toSim(clientX, clientY);
      if (!sim || !pt) return;
      const tool = DRAW_TOOLS.find((t) => t.id === drawToolRef.current);
      if (!tool) return;
      sim.paint(pt[0], pt[1], tool.material, brushSizeRef.current);
    },
    [toSim],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      // Right-click = place bomb
      if (e.button === 2) return;
      (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);
      isDrawing.current = true;
      paintAt(e.clientX, e.clientY);
    },
    [paintAt],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!isDrawing.current) return;
      paintAt(e.clientX, e.clientY);
    },
    [paintAt],
  );

  const onPointerUp = useCallback(() => {
    isDrawing.current = false;
  }, []);

  const onContextMenu = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      e.preventDefault();
      const sim = simRef.current;
      const pt = toSim(e.clientX, e.clientY);
      if (!sim || !pt) return;
      const bomb = BOMB_TOOLS.find((b) => b.id === bombToolRef.current);
      if (!bomb) return;
      sim.place_bomb(pt[0], pt[1], bomb.type);
      setBombCount(sim.bomb_count());
    },
    [toSim],
  );

  // -------------------------------------------------------------------------
  // Actions
  // -------------------------------------------------------------------------
  const handleDetonate = useCallback(() => {
    const sim = simRef.current;
    if (!sim) return;
    sim.detonate_all();
    setIsDetonated(true);
    setBombCount(0);
  }, []);

  const handleReset = useCallback(() => {
    const sim = simRef.current;
    if (!sim) return;
    sim.clear();
    setIsDetonated(false);
    setHudKinetic(0);
    setHudTemp(0);
    setHudDestroyed(0);
    setBombCount(0);
  }, []);

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

      {status === "running" && (
        <>
          {/* Toolbar */}
          <div className="flex flex-wrap justify-center gap-2">
            {/* Material tools */}
            <div className="flex rounded-lg overflow-hidden border border-zinc-700">
              {DRAW_TOOLS.map((tool) => (
                <button
                  key={tool.id}
                  onClick={() => setDrawTool(tool.id)}
                  className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                    drawTool === tool.id
                      ? "text-white"
                      : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                  }`}
                  style={
                    drawTool === tool.id
                      ? { backgroundColor: tool.color, color: tool.id === "glass" ? "#1a1a2e" : "#fff" }
                      : undefined
                  }
                >
                  {tool.label}
                </button>
              ))}
            </div>

            {/* Brush size */}
            <label className="flex items-center gap-2 text-xs text-zinc-400">
              <span className="shrink-0">Brush</span>
              <input
                type="range"
                min={1}
                max={12}
                step={1}
                value={brushSize}
                onChange={(e) => setBrushSize(Number(e.target.value))}
                className="w-16 sm:w-20 accent-zinc-400"
              />
              <span className="w-5 tabular-nums text-zinc-500">{brushSize}</span>
            </label>
          </div>

          {/* Bomb selector + actions */}
          <div className="flex flex-wrap justify-center gap-2">
            <div className="flex rounded-lg overflow-hidden border border-zinc-700">
              {BOMB_TOOLS.map((bomb) => (
                <button
                  key={bomb.id}
                  onClick={() => setBombTool(bomb.id)}
                  title={bomb.desc}
                  className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                    bombTool === bomb.id
                      ? "text-white"
                      : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                  }`}
                  style={
                    bombTool === bomb.id
                      ? { backgroundColor: bomb.color }
                      : undefined
                  }
                >
                  {bomb.label}
                </button>
              ))}
            </div>

            <button
              onClick={handleDetonate}
              disabled={bombCount === 0}
              className="px-4 py-1.5 rounded-lg text-sm font-medium bg-red-600 hover:bg-red-500 text-white border border-red-500 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Detonate All{bombCount > 0 ? ` (${bombCount})` : ""}
            </button>

            <button
              onClick={handleReset}
              className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-zinc-400 hover:bg-zinc-700 border border-zinc-700 transition-colors cursor-pointer"
            >
              Clear
            </button>
          </div>
        </>
      )}

      {/* Canvas + HUD overlay */}
      <div ref={containerRef} className="w-full max-w-[800px] relative">
        <canvas
          ref={canvasRef}
          className="rounded-lg border border-zinc-700 bg-zinc-900 w-full"
          style={{ touchAction: "none", imageRendering: "pixelated", cursor: "crosshair" }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onContextMenu={onContextMenu}
        />

        {/* HUD overlay */}
        {status === "running" && isDetonated && (
          <div className="absolute top-2 left-2 bg-black/70 backdrop-blur-sm rounded-lg border border-zinc-600 px-3 py-2 pointer-events-none">
            <div className="text-[10px] uppercase tracking-wider text-zinc-400 mb-1 font-medium">
              Blast Telemetry
            </div>
            <div className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs tabular-nums">
              <span className="text-orange-400">Kinetic Force</span>
              <span className="text-zinc-200 text-right">{hudKinetic.toFixed(1)} kN</span>
              <span className="text-red-400">Peak Temp</span>
              <span className="text-zinc-200 text-right">{hudTemp.toFixed(0)} K</span>
              <span className="text-emerald-400">Destroyed</span>
              <span className="text-zinc-200 text-right">{hudDestroyed} px</span>
            </div>
          </div>
        )}

        {/* Bomb placement indicators */}
        {status === "running" && bombCount > 0 && !isDetonated && (
          <div className="absolute bottom-2 right-2 bg-black/60 backdrop-blur-sm rounded-lg border border-zinc-600 px-3 py-1.5 pointer-events-none">
            <span className="text-xs text-zinc-300">
              {bombCount} bomb{bombCount !== 1 ? "s" : ""} placed
            </span>
          </div>
        )}
      </div>

      {status === "running" && (
        <p className="text-zinc-500 text-xs text-center px-2">
          Left-click to draw &middot; Right-click to place bomb &middot;
          Hit &ldquo;Detonate All&rdquo; and watch the physics
        </p>
      )}
    </div>
  );
}
