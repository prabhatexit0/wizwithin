import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Palette constants (must match Rust PALETTE_* constants)
// ---------------------------------------------------------------------------
const PALETTE_FIRE = 0;
const PALETTE_ELECTRIC = 1;
const PALETTE_BW = 2;
const PALETTE_OCEAN = 3;

interface PaletteOption {
  label: string;
  value: number;
  swatch: string; // Tailwind gradient or bg class for visual indicator
}

const PALETTES: PaletteOption[] = [
  { label: "Fire", value: PALETTE_FIRE, swatch: "bg-gradient-to-r from-red-700 via-orange-500 to-yellow-300" },
  { label: "Electric", value: PALETTE_ELECTRIC, swatch: "bg-gradient-to-r from-blue-500 via-fuchsia-500 to-cyan-400" },
  { label: "B&W", value: PALETTE_BW, swatch: "bg-gradient-to-r from-black to-white" },
  { label: "Ocean", value: PALETTE_OCEAN, swatch: "bg-gradient-to-r from-blue-900 via-cyan-500 to-white" },
];

// Zoom factor per wheel "click" (deltaY of ~100)
const ZOOM_SENSITIVITY = 1.1;

export default function FractalExplorer() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const [palette, setPalette] = useState(PALETTE_FIRE);
  const [coords, setCoords] = useState({ x: -0.5, y: 0.0, scale: 0 });

  // Refs for the animation/interaction closures
  const fractalRef = useRef<InstanceType<typeof import("@fractal_engine").Fractal> | null>(null);
  const wasmMemoryRef = useRef<WebAssembly.Memory | null>(null);
  const dirtyRef = useRef(true); // whether we need to re-render
  const paletteRef = useRef(palette);
  paletteRef.current = palette;

  // Multi-pointer state (supports 1-finger pan + 2-finger pinch-to-zoom)
  const pointersRef = useRef<Map<number, { x: number; y: number }>>(new Map());
  const pinchDistRef = useRef(0);

  // -----------------------------------------------------------------------
  // Palette change → mark dirty + push to Rust
  // -----------------------------------------------------------------------
  useEffect(() => {
    if (fractalRef.current) {
      fractalRef.current.set_palette(palette);
      dirtyRef.current = true;
    }
  }, [palette]);

  // -----------------------------------------------------------------------
  // Coordinate → screen mapping helper (for display)
  // -----------------------------------------------------------------------
  const updateCoords = useCallback(() => {
    const f = fractalRef.current;
    if (!f) return;
    setCoords({
      x: f.center_x(),
      y: f.center_y(),
      scale: f.scale(),
    });
  }, []);

  // -----------------------------------------------------------------------
  // Boot WASM & start render loop
  // -----------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;
    let resizeObserver: ResizeObserver | null = null;

    async function boot() {
      try {
        const wasm = await import("@fractal_engine");
        const exports = await wasm.default();
        if (cancelled) return;

        const wasmMemory: WebAssembly.Memory = exports.memory;
        wasmMemoryRef.current = wasmMemory;

        const canvas = canvasRef.current!;
        const rect = canvas.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;
        const pixelW = Math.floor(rect.width * dpr);
        const pixelH = Math.floor(rect.height * dpr);
        canvas.width = pixelW;
        canvas.height = pixelH;

        const fractal = new wasm.Fractal(pixelW, pixelH);
        fractalRef.current = fractal;
        dirtyRef.current = true;

        const ctx = canvas.getContext("2d")!;

        // Observe container resizing to keep canvas sharp
        resizeObserver = new ResizeObserver((entries) => {
          for (const entry of entries) {
            const { width, height } = entry.contentRect;
            const dpr = window.devicePixelRatio || 1;
            const pw = Math.floor(width * dpr);
            const ph = Math.floor(height * dpr);
            if (pw > 0 && ph > 0 && (pw !== canvas.width || ph !== canvas.height)) {
              canvas.width = pw;
              canvas.height = ph;
              fractal.resize(pw, ph);
              dirtyRef.current = true;
            }
          }
        });
        resizeObserver.observe(canvas);

        setStatus("running");
        updateCoords();

        // -- Render loop (only re-renders when dirty) -------------------------
        function frame() {
          if (cancelled) return;

          if (dirtyRef.current) {
            dirtyRef.current = false;

            fractal.render();

            const ptr = fractal.buffer_ptr();
            const len = fractal.buffer_len();
            const pixels = new Uint8ClampedArray(wasmMemory.buffer, ptr, len);
            const imageData = new ImageData(pixels, canvas.width, canvas.height);
            ctx.putImageData(imageData, 0, 0);
          }

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot fractal engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      resizeObserver?.disconnect();
      if (fractalRef.current) {
        fractalRef.current.free();
        fractalRef.current = null;
      }
    };
  }, [updateCoords]);

  // -----------------------------------------------------------------------
  // Pointer (mouse/touch) handlers — 1-finger pan + 2-finger pinch-to-zoom
  // -----------------------------------------------------------------------
  const onPointerDown = useCallback((e: React.PointerEvent) => {
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const ptrs = pointersRef.current;
    ptrs.set(e.pointerId, { x: e.clientX, y: e.clientY });

    // When a second finger lands, record initial pinch distance
    if (ptrs.size === 2) {
      const [a, b] = [...ptrs.values()];
      pinchDistRef.current = Math.hypot(b.x - a.x, b.y - a.y);
    }
  }, []);

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      const ptrs = pointersRef.current;
      const prev = ptrs.get(e.pointerId);
      if (!prev || !fractalRef.current) return;

      const dpr = window.devicePixelRatio || 1;

      if (ptrs.size === 1) {
        // Single finger/mouse → pan
        const dx = e.clientX - prev.x;
        const dy = e.clientY - prev.y;
        ptrs.set(e.pointerId, { x: e.clientX, y: e.clientY });

        fractalRef.current.pan(dx * dpr, dy * dpr);
        dirtyRef.current = true;
        updateCoords();
      } else if (ptrs.size === 2) {
        // Two fingers → pinch-to-zoom (centered on midpoint)
        ptrs.set(e.pointerId, { x: e.clientX, y: e.clientY });
        const [a, b] = [...ptrs.values()];
        const newDist = Math.hypot(b.x - a.x, b.y - a.y);
        const oldDist = pinchDistRef.current;

        if (oldDist > 0 && newDist > 0) {
          const factor = newDist / oldDist;
          const canvas = canvasRef.current!;
          const rect = canvas.getBoundingClientRect();
          // Zoom toward the midpoint between the two fingers
          const mx = ((a.x + b.x) / 2 - rect.left) * dpr;
          const my = ((a.y + b.y) / 2 - rect.top) * dpr;
          fractalRef.current.zoom(factor, mx, my);
          dirtyRef.current = true;
          updateCoords();
        }

        pinchDistRef.current = newDist;
      }
    },
    [updateCoords],
  );

  const onPointerUp = useCallback((e: React.PointerEvent) => {
    pointersRef.current.delete(e.pointerId);
    // If one finger remains after lifting, reset pinch state
    if (pointersRef.current.size < 2) {
      pinchDistRef.current = 0;
    }
  }, []);

  // -----------------------------------------------------------------------
  // Wheel handler — zoom toward cursor
  // -----------------------------------------------------------------------
  const onWheel = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      if (!fractalRef.current) return;

      const canvas = canvasRef.current!;
      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;

      // Screen position in device pixels
      const sx = (e.clientX - rect.left) * dpr;
      const sy = (e.clientY - rect.top) * dpr;

      // Each "notch" of the wheel zooms by ZOOM_SENSITIVITY
      const factor = e.deltaY < 0
        ? ZOOM_SENSITIVITY
        : 1.0 / ZOOM_SENSITIVITY;

      fractalRef.current.zoom(factor, sx, sy);
      dirtyRef.current = true;
      updateCoords();
    },
    [updateCoords],
  );

  // -----------------------------------------------------------------------
  // Reset view
  // -----------------------------------------------------------------------
  const onReset = useCallback(() => {
    if (!fractalRef.current) return;
    fractalRef.current.reset();
    dirtyRef.current = true;
    updateCoords();
  }, [updateCoords]);

  // -----------------------------------------------------------------------
  // Zoom level for display (1x = default view)
  // -----------------------------------------------------------------------
  const zoomLevel = coords.scale > 0
    ? (3.5 / (canvasRef.current?.width || 800)) / coords.scale
    : 1;

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

      {/* Controls */}
      <div className="flex gap-2 flex-wrap justify-center items-center">
        {PALETTES.map((p) => (
          <button
            key={p.value}
            onClick={() => setPalette(p.value)}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm transition-colors ${
              palette === p.value
                ? "ring-2 ring-emerald-400 bg-zinc-700 text-zinc-100"
                : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
            }`}
          >
            <span className={`inline-block w-6 h-3 rounded-sm ${p.swatch}`} />
            {p.label}
          </button>
        ))}

        <button
          onClick={onReset}
          className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-zinc-400 hover:bg-zinc-700 transition-colors"
        >
          Reset
        </button>
      </div>

      {/* Canvas — fills container width, 4:3 aspect ratio */}
      <canvas
        ref={canvasRef}
        className="rounded-lg border border-zinc-700 bg-black touch-none w-full max-w-[800px] aspect-[4/3] cursor-grab active:cursor-grabbing"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
        onPointerCancel={onPointerUp}
        onWheel={onWheel}
      />

      {/* Status bar */}
      {status === "running" && (
        <div className="text-zinc-500 text-xs text-center px-2 space-y-0.5">
          <p>
            Center: ({coords.x.toFixed(8)}, {coords.y.toFixed(8)}i)
            {" "}&middot;{" "}
            Zoom: {zoomLevel >= 1000 ? `${(zoomLevel / 1000).toFixed(1)}k` : zoomLevel.toFixed(1)}x
          </p>
          <p className="hidden sm:block">
            Drag to pan &middot; Scroll to zoom toward cursor
          </p>
          <p className="sm:hidden">
            Drag to pan &middot; Pinch to zoom
          </p>
        </div>
      )}
    </div>
  );
}
