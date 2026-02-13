import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const SIM_W = 640;
const SIM_H = 400;

const EMPTY = 0;
const WOOD = 1;
const STONE = 2;
const STEEL = 3;
const GLASS = 4;

const BOMB_C4 = 0;
const BOMB_THERMITE = 1;
const BOMB_DIRTY = 2;

type DrawTool = "wood" | "stone" | "steel" | "glass" | "eraser";
type BombTool = "c4" | "thermite" | "dirty";
type InteractionMode = "draw" | "bomb";

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
// Procedural Web Audio sounds
// ---------------------------------------------------------------------------

let audioCtx: AudioContext | null = null;

function getAudioCtx(): AudioContext {
  if (!audioCtx) audioCtx = new AudioContext();
  if (audioCtx.state === "suspended") audioCtx.resume();
  return audioCtx;
}

function playExplosionSound(mask: number) {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;

  // C4 — deep bass boom + noise crack
  if (mask & 1) {
    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.setValueAtTime(70, now);
    osc.frequency.exponentialRampToValueAtTime(20, now + 0.6);
    const oscGain = ctx.createGain();
    oscGain.gain.setValueAtTime(0.5, now);
    oscGain.gain.exponentialRampToValueAtTime(0.001, now + 0.8);
    osc.connect(oscGain).connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.8);

    // Noise crack
    const nLen = Math.floor(ctx.sampleRate * 0.25);
    const nBuf = ctx.createBuffer(1, nLen, ctx.sampleRate);
    const nd = nBuf.getChannelData(0);
    for (let i = 0; i < nLen; i++) nd[i] = (Math.random() * 2 - 1) * Math.exp(-i / (ctx.sampleRate * 0.04));
    const ns = ctx.createBufferSource();
    ns.buffer = nBuf;
    const nf = ctx.createBiquadFilter();
    nf.type = "lowpass";
    nf.frequency.setValueAtTime(1200, now);
    nf.frequency.exponentialRampToValueAtTime(80, now + 0.3);
    const ng = ctx.createGain();
    ng.gain.setValueAtTime(0.35, now);
    ng.gain.exponentialRampToValueAtTime(0.001, now + 0.3);
    ns.connect(nf).connect(ng).connect(ctx.destination);
    ns.start(now);
  }

  // Thermite — hiss + crackle
  if (mask & 2) {
    const nLen = Math.floor(ctx.sampleRate * 1.5);
    const nBuf = ctx.createBuffer(1, nLen, ctx.sampleRate);
    const nd = nBuf.getChannelData(0);
    for (let i = 0; i < nLen; i++) {
      const env = Math.exp(-i / (ctx.sampleRate * 0.5));
      nd[i] = (Math.random() * 2 - 1) * env * (Math.random() > 0.97 ? 3 : 1);
    }
    const ns = ctx.createBufferSource();
    ns.buffer = nBuf;
    const bp = ctx.createBiquadFilter();
    bp.type = "bandpass";
    bp.frequency.setValueAtTime(3000, now);
    bp.Q.setValueAtTime(1.5, now);
    const ng = ctx.createGain();
    ng.gain.setValueAtTime(0.2, now);
    ng.gain.exponentialRampToValueAtTime(0.001, now + 1.5);
    ns.connect(bp).connect(ng).connect(ctx.destination);
    ns.start(now);

    // Low body
    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.setValueAtTime(150, now);
    osc.frequency.exponentialRampToValueAtTime(40, now + 0.4);
    const og = ctx.createGain();
    og.gain.setValueAtTime(0.15, now);
    og.gain.exponentialRampToValueAtTime(0.001, now + 0.5);
    osc.connect(og).connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.5);
  }

  // Dirty bomb — muffled boom + geiger clicks
  if (mask & 4) {
    const osc = ctx.createOscillator();
    osc.type = "sine";
    osc.frequency.setValueAtTime(50, now);
    osc.frequency.exponentialRampToValueAtTime(15, now + 0.5);
    const lp = ctx.createBiquadFilter();
    lp.type = "lowpass";
    lp.frequency.value = 200;
    const og = ctx.createGain();
    og.gain.setValueAtTime(0.4, now);
    og.gain.exponentialRampToValueAtTime(0.001, now + 0.7);
    osc.connect(lp).connect(og).connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.7);

    // Geiger clicks
    for (let c = 0; c < 12; c++) {
      const t = now + 0.3 + Math.random() * 1.5;
      const cl = Math.floor(ctx.sampleRate * 0.008);
      const cb = ctx.createBuffer(1, cl, ctx.sampleRate);
      const cd = cb.getChannelData(0);
      for (let i = 0; i < cl; i++) cd[i] = (Math.random() * 2 - 1) * Math.exp(-i / (cl * 0.2));
      const cs = ctx.createBufferSource();
      cs.buffer = cb;
      const cg = ctx.createGain();
      cg.gain.setValueAtTime(0.15, t);
      cg.gain.exponentialRampToValueAtTime(0.001, t + 0.02);
      cs.connect(cg).connect(ctx.destination);
      cs.start(t);
    }
  }
}

function playPlaceBombSound() {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;
  const osc = ctx.createOscillator();
  osc.type = "sine";
  osc.frequency.setValueAtTime(200, now);
  osc.frequency.exponentialRampToValueAtTime(80, now + 0.1);
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.12, now);
  g.gain.exponentialRampToValueAtTime(0.001, now + 0.12);
  osc.connect(g).connect(ctx.destination);
  osc.start(now);
  osc.stop(now + 0.12);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function BlastLab() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");

  const [mode, setMode] = useState<InteractionMode>("draw");
  const [drawTool, setDrawTool] = useState<DrawTool>("steel");
  const [bombTool, setBombTool] = useState<BombTool>("c4");
  const [brushSize, setBrushSize] = useState(5);
  const [bombCount, setBombCount] = useState(0);
  const [isDetonated, setIsDetonated] = useState(false);

  // Animated HUD values (lerped toward actuals)
  const [hudKinetic, setHudKinetic] = useState(0);
  const [hudTemp, setHudTemp] = useState(0);
  const [hudDestroyed, setHudDestroyed] = useState(0);
  const [hudEnergy, setHudEnergy] = useState(0);

  // Refs for RAF loop
  const simRef = useRef<InstanceType<typeof import("@blast_lab").BlastLabSim> | null>(null);
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const drawToolRef = useRef(drawTool);
  drawToolRef.current = drawTool;
  const bombToolRef = useRef(bombTool);
  bombToolRef.current = bombTool;
  const brushSizeRef = useRef(brushSize);
  brushSizeRef.current = brushSize;
  const isDetonatedRef = useRef(isDetonated);
  isDetonatedRef.current = isDetonated;

  // Screen shake + flash (managed in RAF)
  const shakeRef = useRef(0);
  const flashRef = useRef(0);
  const flashColorRef = useRef("255,255,255");

  // Animated counter refs
  const displayKinetic = useRef(0);
  const displayTemp = useRef(0);
  const displayDestroyed = useRef(0);
  const displayEnergy = useRef(0);

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
        canvas.width = SIM_W;
        canvas.height = SIM_H;

        setStatus("running");

        let frameCount = 0;

        function frame() {
          if (cancelled) return;

          if (isDetonatedRef.current) {
            sim.tick();
          }

          sim.render();

          const ptr = sim.pixels_ptr();
          const len = sim.pixels_len();
          const pixels = new Uint8ClampedArray(wasmMemory.buffer, ptr, len);
          const imageData = new ImageData(pixels, SIM_W, SIM_H);
          ctx.putImageData(imageData, 0, 0);

          // Flash overlay (drawn after putImageData since fillRect respects alpha)
          if (flashRef.current > 0.01) {
            ctx.globalAlpha = flashRef.current;
            ctx.fillStyle = `rgb(${flashColorRef.current})`;
            ctx.fillRect(0, 0, SIM_W, SIM_H);
            ctx.globalAlpha = 1;
            flashRef.current *= 0.88;
          }

          // Screen shake
          if (shakeRef.current > 0.5) {
            const dx = (Math.random() - 0.5) * shakeRef.current;
            const dy = (Math.random() - 0.5) * shakeRef.current;
            canvas.style.transform = `translate(${dx}px, ${dy}px)`;
            shakeRef.current *= 0.90;
          } else if (shakeRef.current > 0) {
            canvas.style.transform = "";
            shakeRef.current = 0;
          }

          // Update animated HUD counters every 4 frames
          frameCount++;
          if (frameCount % 4 === 0) {
            const tk = sim.stats_peak_kinetic();
            const tt = sim.stats_peak_temp();
            const td = sim.stats_pixels_destroyed();
            const te = sim.stats_total_energy();

            // Lerp displayed values toward actual
            displayKinetic.current += (tk - displayKinetic.current) * 0.15;
            displayTemp.current += (tt - displayTemp.current) * 0.15;
            displayDestroyed.current += (td - displayDestroyed.current) * 0.2;
            displayEnergy.current += (te - displayEnergy.current) * 0.12;

            setHudKinetic(displayKinetic.current);
            setHudTemp(displayTemp.current);
            setHudDestroyed(Math.round(displayDestroyed.current));
            setHudEnergy(displayEnergy.current);
            setBombCount(sim.bomb_count());
          }

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);
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
      return [
        Math.floor((clientX - rect.left) * SIM_W / rect.width),
        Math.floor((clientY - rect.top) * SIM_H / rect.height),
      ];
    },
    [],
  );

  // -------------------------------------------------------------------------
  // Brush interpolation — fill gaps between pointer events
  // -------------------------------------------------------------------------
  const lastPaint = useRef<[number, number] | null>(null);

  const paintAt = useCallback(
    (clientX: number, clientY: number) => {
      const sim = simRef.current;
      const pt = toSim(clientX, clientY);
      if (!sim || !pt) return;
      const tool = DRAW_TOOLS.find((t) => t.id === drawToolRef.current);
      if (!tool) return;

      const lp = lastPaint.current;
      if (lp) {
        // Interpolate between last and current position
        const dx = pt[0] - lp[0];
        const dy = pt[1] - lp[1];
        const dist = Math.sqrt(dx * dx + dy * dy);
        const step = Math.max(1, brushSizeRef.current * 0.5);
        const steps = Math.ceil(dist / step);
        for (let i = 0; i < steps; i++) {
          const t = i / Math.max(1, steps);
          const ix = Math.round(lp[0] + dx * t);
          const iy = Math.round(lp[1] + dy * t);
          sim.paint(ix, iy, tool.material, brushSizeRef.current);
        }
      }

      sim.paint(pt[0], pt[1], tool.material, brushSizeRef.current);
      lastPaint.current = pt;
    },
    [toSim],
  );

  const placeBombAt = useCallback(
    (clientX: number, clientY: number) => {
      const sim = simRef.current;
      const pt = toSim(clientX, clientY);
      if (!sim || !pt) return;
      const bomb = BOMB_TOOLS.find((b) => b.id === bombToolRef.current);
      if (!bomb) return;
      sim.place_bomb(pt[0], pt[1], bomb.type);
      setBombCount(sim.bomb_count());
      playPlaceBombSound();
    },
    [toSim],
  );

  // -------------------------------------------------------------------------
  // Pointer events
  // -------------------------------------------------------------------------
  const isDrawing = useRef(false);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (e.button === 2) return;
      (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);

      if (modeRef.current === "bomb") {
        placeBombAt(e.clientX, e.clientY);
      } else {
        isDrawing.current = true;
        lastPaint.current = null;
        paintAt(e.clientX, e.clientY);
      }
    },
    [paintAt, placeBombAt],
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
    lastPaint.current = null;
  }, []);

  const onContextMenu = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      e.preventDefault();
      placeBombAt(e.clientX, e.clientY);
    },
    [placeBombAt],
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

    // Trigger sound
    const mask = sim.detonated_mask();
    playExplosionSound(mask);

    // Screen shake + flash
    shakeRef.current = 10;
    flashRef.current = 0.7;
    // Flash color based on dominant bomb type
    if (mask & 4) flashColorRef.current = "80,255,60";
    else if (mask & 2) flashColorRef.current = "255,120,40";
    else flashColorRef.current = "255,220,150";
  }, []);

  const handleReset = useCallback(() => {
    const sim = simRef.current;
    if (!sim) return;
    sim.clear();
    setIsDetonated(false);
    setHudKinetic(0);
    setHudTemp(0);
    setHudDestroyed(0);
    setHudEnergy(0);
    setBombCount(0);
    displayKinetic.current = 0;
    displayTemp.current = 0;
    displayDestroyed.current = 0;
    displayEnergy.current = 0;
    shakeRef.current = 0;
    flashRef.current = 0;
    if (canvasRef.current) canvasRef.current.style.transform = "";
  }, []);

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------
  return (
    <div className="flex flex-col items-center gap-3 w-full">
      {status === "loading" && (
        <p className="text-zinc-400 text-sm animate-pulse">Loading WASM module&hellip;</p>
      )}
      {status === "error" && (
        <p className="text-red-400 text-sm">Error: {errorMsg}</p>
      )}

      {status === "running" && (
        <>
          {/* Mode toggle + tools */}
          <div className="flex flex-wrap justify-center gap-2">
            <div className="flex rounded-lg overflow-hidden border border-zinc-700">
              <button
                onClick={() => setMode("draw")}
                className={`px-3 py-1.5 text-sm font-medium transition-colors cursor-pointer ${
                  mode === "draw" ? "bg-sky-600 text-white" : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                }`}
              >
                Draw
              </button>
              <button
                onClick={() => setMode("bomb")}
                className={`px-3 py-1.5 text-sm font-medium transition-colors cursor-pointer ${
                  mode === "bomb" ? "bg-red-600 text-white" : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                }`}
              >
                Bomb
              </button>
            </div>

            {mode === "draw" && (
              <>
                <div className="flex rounded-lg overflow-hidden border border-zinc-700">
                  {DRAW_TOOLS.map((tool) => (
                    <button
                      key={tool.id}
                      onClick={() => setDrawTool(tool.id)}
                      className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                        drawTool === tool.id ? "text-white" : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
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

                <label className="flex items-center gap-2 text-xs text-zinc-400">
                  <span className="shrink-0">Brush</span>
                  <input
                    type="range" min={1} max={15} step={1}
                    value={brushSize}
                    onChange={(e) => setBrushSize(Number(e.target.value))}
                    className="w-16 sm:w-20 accent-zinc-400"
                  />
                  <span className="w-5 tabular-nums text-zinc-500">{brushSize}</span>
                </label>
              </>
            )}

            {mode === "bomb" && (
              <div className="flex rounded-lg overflow-hidden border border-zinc-700">
                {BOMB_TOOLS.map((bomb) => (
                  <button
                    key={bomb.id}
                    onClick={() => setBombTool(bomb.id)}
                    title={bomb.desc}
                    className={`px-3 py-1.5 text-sm transition-colors cursor-pointer ${
                      bombTool === bomb.id ? "text-white" : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                    }`}
                    style={bombTool === bomb.id ? { backgroundColor: bomb.color } : undefined}
                  >
                    {bomb.label}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Actions row */}
          <div className="flex flex-wrap justify-center gap-2">
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
      <div className="w-full max-w-[960px] relative overflow-hidden rounded-lg border border-zinc-700">
        <canvas
          ref={canvasRef}
          className="bg-zinc-900 w-full block"
          style={{ touchAction: "none", imageRendering: "pixelated", cursor: mode === "bomb" ? "crosshair" : "default" }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onContextMenu={onContextMenu}
        />

        {/* HUD overlay */}
        {status === "running" && isDetonated && (
          <div className="absolute top-2 left-2 bg-black/75 backdrop-blur-sm rounded-lg border border-zinc-600 px-3 py-2 pointer-events-none select-none">
            <div className="text-[10px] uppercase tracking-wider text-zinc-500 mb-1.5 font-semibold">
              Blast Telemetry
            </div>
            <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs tabular-nums font-mono">
              <span className="text-orange-400">Kinetic</span>
              <span className="text-zinc-100 text-right">{hudKinetic.toFixed(1)} <span className="text-zinc-500">kN</span></span>
              <span className="text-red-400">Temp</span>
              <span className="text-zinc-100 text-right">{hudTemp.toFixed(0)} <span className="text-zinc-500">K</span></span>
              <span className="text-emerald-400">Destroyed</span>
              <span className="text-zinc-100 text-right">{hudDestroyed} <span className="text-zinc-500">px</span></span>
              <span className="text-yellow-400">Energy</span>
              <span className="text-zinc-100 text-right">{hudEnergy.toFixed(0)} <span className="text-zinc-500">J</span></span>
            </div>
          </div>
        )}

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
          {mode === "draw" ? "Tap/drag to draw" : "Tap to place bomb"} &middot;
          Right-click always places bomb &middot;
          Toggle Draw/Bomb above
        </p>
      )}
    </div>
  );
}
