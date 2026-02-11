import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Rendering constants — authentic 1992 blocky look
// ---------------------------------------------------------------------------
const VIEW_W = 320;
const VIEW_H = 200;

export default function Raycaster() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const [fov, setFov] = useState(0.66);
  const [trippiness, setTrippiness] = useState(1.0);
  const [showMinimap, setShowMinimap] = useState(true);

  // Refs that the animation loop closes over
  const fovRef = useRef(fov);
  fovRef.current = fov;
  const trippinessRef = useRef(trippiness);
  trippinessRef.current = trippiness;
  const showMinimapRef = useRef(showMinimap);
  showMinimapRef.current = showMinimap;

  const worldRef = useRef<InstanceType<typeof import("@raycaster_engine").World> | null>(null);

  // Track which keys are currently held down (for smooth movement)
  const keysRef = useRef<Set<string>>(new Set());

  // -----------------------------------------------------------------------
  // Keyboard event handlers — track active keys, not individual presses
  // -----------------------------------------------------------------------
  const onKeyDown = useCallback((e: KeyboardEvent) => {
    const key = e.key.toLowerCase();
    if (["w", "a", "s", "d", "q", "e", "arrowup", "arrowdown", "arrowleft", "arrowright"].includes(key)) {
      e.preventDefault();
      keysRef.current.add(key);
    }
  }, []);

  const onKeyUp = useCallback((e: KeyboardEvent) => {
    keysRef.current.delete(e.key.toLowerCase());
  }, []);

  // -----------------------------------------------------------------------
  // Mobile touch controls — simulate key hold via pointer events
  // -----------------------------------------------------------------------
  const padDown = useCallback((key: string) => {
    keysRef.current.add(key);
  }, []);

  const padUp = useCallback((key: string) => {
    keysRef.current.delete(key);
  }, []);

  // -----------------------------------------------------------------------
  // Boot WASM & start render loop
  // -----------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    let rafId = 0;

    async function boot() {
      try {
        const wasm = await import("@raycaster_engine");
        const exports = await wasm.default();
        if (cancelled) return;

        const wasmMemory: WebAssembly.Memory = exports.memory;
        const world = new wasm.World(VIEW_W, VIEW_H);
        worldRef.current = world;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;

        // Off-screen canvas at native resolution
        const offscreen = document.createElement("canvas");
        offscreen.width = VIEW_W;
        offscreen.height = VIEW_H;
        const offCtx = offscreen.getContext("2d")!;
        ctx.imageSmoothingEnabled = false;

        setStatus("running");

        let lastTime = performance.now();

        function frame(now: number) {
          if (cancelled) return;

          const dt = Math.min((now - lastTime) / 1000, 0.1);
          lastTime = now;

          const keys = keysRef.current;
          const moveSpeed = 3.5 * dt;
          const rotSpeed = 2.5 * dt;

          // Apply movement based on held keys
          if (keys.has("w") || keys.has("arrowup")) world.move_forward(moveSpeed);
          if (keys.has("s") || keys.has("arrowdown")) world.move_backward(moveSpeed);
          if (keys.has("a")) world.strafe_left(moveSpeed);
          if (keys.has("d")) world.strafe_right(moveSpeed);
          if (keys.has("arrowleft") || keys.has("q")) world.rotate_left(rotSpeed);
          if (keys.has("arrowright") || keys.has("e")) world.rotate_right(rotSpeed);

          // Update FOV & minimap visibility
          world.set_fov(fovRef.current);
          world.set_show_minimap(showMinimapRef.current);

          // Render scene
          const elapsed = now / 1000;
          world.render(elapsed, trippinessRef.current);

          // Zero-copy: build typed array view into WASM memory
          const ptr = world.pixels_ptr();
          const len = world.pixels_len();
          const pixels = new Uint8ClampedArray(wasmMemory.buffer, ptr, len);

          const imageData = new ImageData(pixels, VIEW_W, VIEW_H);
          offCtx.putImageData(imageData, 0, 0);

          // Scale up to display canvas
          ctx.drawImage(offscreen, 0, 0, canvas.width, canvas.height);

          rafId = requestAnimationFrame(frame);
        }

        rafId = requestAnimationFrame(frame);
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to boot raycaster engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    // Register key listeners
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    boot();

    return () => {
      cancelled = true;
      cancelAnimationFrame(rafId);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      keysRef.current.clear();
      if (worldRef.current) {
        worldRef.current.free();
        worldRef.current = null;
      }
    };
  }, [onKeyDown, onKeyUp]);

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------
  return (
    <div className="flex flex-col items-center gap-4 w-full">
      {status === "loading" && (
        <p className="text-zinc-400 text-sm animate-pulse">Loading WASM module...</p>
      )}
      {status === "error" && (
        <p className="text-red-400 text-sm">Error: {errorMsg}</p>
      )}

      {/* Canvas — pixelated upscale for retro look */}
      <canvas
        ref={canvasRef}
        width={VIEW_W * 2}
        height={VIEW_H * 2}
        className="rounded-lg border border-zinc-700 bg-[#1c1c24] w-full max-w-2xl touch-none"
        style={{
          imageRendering: "pixelated",
          aspectRatio: `${VIEW_W} / ${VIEW_H}`,
        }}
      />

      {/* Controls */}
      {status === "running" && (
        <div className="flex flex-col gap-3 w-full max-w-2xl">
          {/* Sliders */}
          <div className="flex flex-wrap gap-4 justify-center">
            <label className="flex items-center gap-2 text-sm text-zinc-400">
              <span className="w-28">FOV</span>
              <input
                type="range"
                min="0.3"
                max="1.4"
                step="0.01"
                value={fov}
                onChange={(e) => setFov(parseFloat(e.target.value))}
                className="w-32 accent-emerald-400"
              />
              <span className="w-10 text-right font-mono text-xs text-zinc-500">
                {fov.toFixed(2)}
              </span>
            </label>

            <label className="flex items-center gap-2 text-sm text-zinc-400">
              <span className="w-28">Trippiness</span>
              <input
                type="range"
                min="0"
                max="5"
                step="0.05"
                value={trippiness}
                onChange={(e) => setTrippiness(parseFloat(e.target.value))}
                className="w-32 accent-fuchsia-400"
              />
              <span className="w-10 text-right font-mono text-xs text-zinc-500">
                {trippiness.toFixed(1)}
              </span>
            </label>

            <button
              onClick={() => setShowMinimap((v) => !v)}
              className={`px-3 py-1 rounded text-sm font-mono transition-colors ${
                showMinimap
                  ? "bg-zinc-700 text-zinc-300"
                  : "bg-zinc-800 text-zinc-500"
              }`}
            >
              Map {showMinimap ? "ON" : "OFF"}
            </button>
          </div>

          {/* Mobile on-screen controls — visible on small screens */}
          <div
            className="flex sm:hidden justify-between w-full px-2"
            style={{ touchAction: "manipulation" }}
            onContextMenu={(e) => e.preventDefault()}
          >
            {/* Left: movement D-pad */}
            <div className="grid grid-cols-3 grid-rows-3 gap-1 w-fit">
              {/* Row 1: forward */}
              <div />
              <button
                onPointerDown={() => padDown("w")}
                onPointerUp={() => padUp("w")}
                onPointerLeave={() => padUp("w")}
                onPointerCancel={() => padUp("w")}
                className="w-12 h-12 rounded bg-zinc-700 active:bg-emerald-500 active:text-zinc-900 text-zinc-300 text-lg font-bold flex items-center justify-center select-none touch-manipulation"
                aria-label="Move forward"
              >
                &#9650;
              </button>
              <div />
              {/* Row 2: strafe left, backward, strafe right */}
              <button
                onPointerDown={() => padDown("a")}
                onPointerUp={() => padUp("a")}
                onPointerLeave={() => padUp("a")}
                onPointerCancel={() => padUp("a")}
                className="w-12 h-12 rounded bg-zinc-700 active:bg-emerald-500 active:text-zinc-900 text-zinc-300 text-lg font-bold flex items-center justify-center select-none touch-manipulation"
                aria-label="Strafe left"
              >
                &#9664;
              </button>
              <button
                onPointerDown={() => padDown("s")}
                onPointerUp={() => padUp("s")}
                onPointerLeave={() => padUp("s")}
                onPointerCancel={() => padUp("s")}
                className="w-12 h-12 rounded bg-zinc-700 active:bg-emerald-500 active:text-zinc-900 text-zinc-300 text-lg font-bold flex items-center justify-center select-none touch-manipulation"
                aria-label="Move backward"
              >
                &#9660;
              </button>
              <button
                onPointerDown={() => padDown("d")}
                onPointerUp={() => padUp("d")}
                onPointerLeave={() => padUp("d")}
                onPointerCancel={() => padUp("d")}
                className="w-12 h-12 rounded bg-zinc-700 active:bg-emerald-500 active:text-zinc-900 text-zinc-300 text-lg font-bold flex items-center justify-center select-none touch-manipulation"
                aria-label="Strafe right"
              >
                &#9654;
              </button>
              <div />
            </div>

            {/* Right: rotation buttons */}
            <div className="flex items-center gap-2">
              <button
                onPointerDown={() => padDown("q")}
                onPointerUp={() => padUp("q")}
                onPointerLeave={() => padUp("q")}
                onPointerCancel={() => padUp("q")}
                className="w-14 h-14 rounded-lg bg-zinc-700 active:bg-fuchsia-500 active:text-zinc-900 text-zinc-300 text-sm font-bold flex items-center justify-center select-none touch-manipulation"
                aria-label="Rotate left"
              >
                &#8630;
              </button>
              <button
                onPointerDown={() => padDown("e")}
                onPointerUp={() => padUp("e")}
                onPointerLeave={() => padUp("e")}
                onPointerCancel={() => padUp("e")}
                className="w-14 h-14 rounded-lg bg-zinc-700 active:bg-fuchsia-500 active:text-zinc-900 text-zinc-300 text-sm font-bold flex items-center justify-center select-none touch-manipulation"
                aria-label="Rotate right"
              >
                &#8631;
              </button>
            </div>
          </div>

          {/* Key legend — visible on larger screens with keyboards */}
          <div className="hidden sm:flex flex-wrap gap-x-4 gap-y-1 justify-center text-xs text-zinc-500">
            <span>
              <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono">W</kbd>{" "}
              <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono">S</kbd>{" "}
              Move
            </span>
            <span>
              <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono">A</kbd>{" "}
              <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono">D</kbd>{" "}
              Strafe
            </span>
            <span>
              <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono">Q</kbd>{" "}
              <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono">E</kbd>{" "}
              / Arrows — Rotate
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
