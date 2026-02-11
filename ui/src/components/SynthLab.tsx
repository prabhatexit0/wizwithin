import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const BUFFER_SIZE = 2048; // samples per audio callback (~46 ms at 44.1 kHz)
const FREQ_MIN = 100;
const FREQ_MAX = 800;
const SCOPE_LINE_COLOR = "#34d399"; // emerald-400

// Waveform types (must match Rust constants)
const WAVEFORMS = [
  { label: "Sine", value: 0 },
  { label: "Square", value: 1 },
  { label: "Saw", value: 2 },
  { label: "Triangle", value: 3 },
] as const;

export default function SynthLab() {
  const padRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "playing" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const [waveform, setWaveform] = useState(0);
  const [freqDisplay, setFreqDisplay] = useState(440);
  const [gainDisplay, setGainDisplay] = useState(0);

  // Refs for the audio/WASM objects that live outside React's render cycle.
  const synthRef = useRef<InstanceType<typeof import("@synth_engine").Synth> | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const wasmMemoryRef = useRef<WebAssembly.Memory | null>(null);
  const waveformRef = useRef(0);
  waveformRef.current = waveform;

  // Track whether the pointer is actively down on the pad.
  const activeRef = useRef(false);

  // -------------------------------------------------------------------
  // Boot: load WASM module (but don't start audio yet — need user gesture)
  // -------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;

    async function boot() {
      try {
        const wasm = await import("@synth_engine");
        const exports = await wasm.default();
        if (cancelled) return;

        wasmMemoryRef.current = exports.memory;

        // We'll create the Synth instance once we know the actual sample rate
        // from the AudioContext (created on first user gesture).  Store the
        // class constructor on the ref for later.
        (wasmMemoryRef as any)._SynthClass = wasm.Synth;

        setStatus("ready");
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to load synth engine:", err);
          setErrorMsg(String(err));
          setStatus("error");
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      processorRef.current?.disconnect();
      audioCtxRef.current?.close();
      synthRef.current?.free();
      synthRef.current = null;
    };
  }, []);

  // -------------------------------------------------------------------
  // Start / resume AudioContext (requires user gesture)
  // -------------------------------------------------------------------
  const ensureAudio = useCallback(() => {
    if (audioCtxRef.current && synthRef.current) {
      // Already initialised — just make sure it's running.
      if (audioCtxRef.current.state === "suspended") {
        audioCtxRef.current.resume();
      }
      return;
    }

    const SynthClass = (wasmMemoryRef as any)._SynthClass;
    if (!SynthClass || !wasmMemoryRef.current) return;

    // Create AudioContext inside a user-gesture handler so browsers allow it.
    const ctx = new AudioContext();
    audioCtxRef.current = ctx;

    const sampleRate = ctx.sampleRate;
    const synth = new SynthClass(sampleRate, BUFFER_SIZE);
    synthRef.current = synth;

    // ScriptProcessorNode: the simplest way to pull samples from WASM.
    // (Deprecated but universally supported; AudioWorklet requires a
    // separate JS file which complicates Vite bundling.)
    const processor = ctx.createScriptProcessor(BUFFER_SIZE, 0, 1);
    processorRef.current = processor;

    const memory = wasmMemoryRef.current;

    processor.onaudioprocess = (e) => {
      const output = e.outputBuffer.getChannelData(0);

      // Ask Rust to fill its internal buffer with the next chunk.
      synth.fill_buffer();

      // Build a Float32Array *view* into WASM linear memory — zero copy.
      const ptr = synth.buffer_ptr();
      const len = synth.buffer_len();
      const wasmBuf = new Float32Array(memory.buffer, ptr, len);

      // Copy into the AudioContext output buffer.
      output.set(wasmBuf);
    };

    processor.connect(ctx.destination);
    setStatus("playing");
  }, []);

  // -------------------------------------------------------------------
  // Map pointer position on the pad to frequency + gain
  // -------------------------------------------------------------------
  const applyPointer = useCallback(
    (clientX: number, clientY: number) => {
      const canvas = padRef.current;
      const synth = synthRef.current;
      if (!canvas || !synth) return;

      const rect = canvas.getBoundingClientRect();
      const nx = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      const ny = Math.max(0, Math.min(1, (clientY - rect.top) / rect.height));

      // X → frequency (left = low, right = high)
      const freq = FREQ_MIN + nx * (FREQ_MAX - FREQ_MIN);
      // Y → gain (top = loud, bottom = silent)
      const gain = 1.0 - ny;

      synth.set_frequency(freq);
      synth.set_gain(gain);

      setFreqDisplay(Math.round(freq));
      setGainDisplay(Math.round(gain * 100));
    },
    [],
  );

  // -------------------------------------------------------------------
  // Pointer event handlers for the Theremin pad
  // -------------------------------------------------------------------
  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      activeRef.current = true;
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      ensureAudio();
      applyPointer(e.clientX, e.clientY);
    },
    [ensureAudio, applyPointer],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!activeRef.current) return;
      applyPointer(e.clientX, e.clientY);
    },
    [applyPointer],
  );

  const onPointerUp = useCallback(() => {
    activeRef.current = false;
    // Silence when finger/mouse is lifted.
    synthRef.current?.set_gain(0.0);
    setGainDisplay(0);
  }, []);

  // -------------------------------------------------------------------
  // Waveform selector — sync to WASM whenever it changes
  // -------------------------------------------------------------------
  useEffect(() => {
    synthRef.current?.set_waveform(waveform);
  }, [waveform]);

  // -------------------------------------------------------------------
  // Oscilloscope: draw the current waveform on the pad canvas
  // -------------------------------------------------------------------
  useEffect(() => {
    const canvas = padRef.current;
    if (!canvas) return;

    let rafId = 0;

    function draw() {
      const ctx = canvas!.getContext("2d");
      if (!ctx) return;

      const w = canvas!.width;
      const h = canvas!.height;

      // Dark background
      ctx.fillStyle = "#18181b"; // zinc-900
      ctx.fillRect(0, 0, w, h);

      // Grid lines (subtle)
      ctx.strokeStyle = "#27272a"; // zinc-800
      ctx.lineWidth = 1;
      for (let i = 1; i < 4; i++) {
        const y = (h / 4) * i;
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }
      for (let i = 1; i < 4; i++) {
        const x = (w / 4) * i;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
        ctx.stroke();
      }

      // Center line
      ctx.strokeStyle = "#3f3f46"; // zinc-700
      ctx.beginPath();
      ctx.moveTo(0, h / 2);
      ctx.lineTo(w, h / 2);
      ctx.stroke();

      // Draw waveform from the WASM buffer
      const synth = synthRef.current;
      const memory = wasmMemoryRef.current;
      if (synth && memory) {
        const ptr = synth.buffer_ptr();
        const len = synth.buffer_len();
        // Re-create view each frame (memory.buffer can detach on growth)
        const samples = new Float32Array(memory.buffer, ptr, len);

        // Show ~3 cycles worth of samples for a clear waveform display.
        // We limit how many samples we plot so the wave is readable.
        const samplesToShow = Math.min(len, 512);

        ctx.strokeStyle = SCOPE_LINE_COLOR;
        ctx.lineWidth = 2;
        ctx.beginPath();

        for (let i = 0; i < samplesToShow; i++) {
          const x = (i / samplesToShow) * w;
          // samples are in [-1, 1]; map to canvas y
          const y = (1 - samples[i]) * 0.5 * h;
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }

        ctx.stroke();
      }

      rafId = requestAnimationFrame(draw);
    }

    rafId = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(rafId);
  }, []);

  // -------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------
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

      {/* Waveform selector */}
      <div className="flex gap-2 flex-wrap justify-center">
        {WAVEFORMS.map((w) => (
          <button
            key={w.value}
            onClick={() => setWaveform(w.value)}
            className={`px-3 py-1.5 rounded-lg text-sm transition-colors ${
              waveform === w.value
                ? "ring-2 ring-emerald-400 bg-zinc-700 text-zinc-100"
                : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
            }`}
          >
            {w.label}
          </button>
        ))}
      </div>

      {/* Readout */}
      <div className="flex gap-4 text-xs text-zinc-400 font-mono">
        <span>Freq: {freqDisplay} Hz</span>
        <span>Vol: {gainDisplay}%</span>
      </div>

      {/* XY Theremin Pad + Oscilloscope */}
      <canvas
        ref={padRef}
        width={512}
        height={320}
        className="rounded-lg border border-zinc-700 bg-zinc-900 touch-none w-full max-w-[512px] cursor-crosshair"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
      />

      {/* Instructions */}
      {(status === "ready" || status === "playing") && (
        <p className="text-zinc-500 text-xs text-center px-2">
          Press &amp; drag on the pad &middot; X&nbsp;=&nbsp;Pitch &middot;
          Y&nbsp;=&nbsp;Volume &middot; Release to silence
        </p>
      )}
    </div>
  );
}
