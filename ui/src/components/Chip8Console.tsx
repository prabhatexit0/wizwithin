import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// CHIP-8 display constants
// ---------------------------------------------------------------------------
const CHIP8_W = 64;
const CHIP8_H = 32;
const SCALE = 10; // each CHIP-8 pixel → 10x10 CSS pixels
const CANVAS_W = CHIP8_W * SCALE;
const CANVAS_H = CHIP8_H * SCALE;

// Foreground / background colours (phosphor green on dark)
const FG = [0x33, 0xff, 0x66]; // #33ff66
const BG = [0x0a, 0x0f, 0x0a]; // near-black green tint

// ---------------------------------------------------------------------------
// Keyboard → CHIP-8 hex keypad mapping
//
//   CHIP-8 keypad:       Keyboard mapping:
//   1 2 3 C              1 2 3 4
//   4 5 6 D              Q W E R
//   7 8 9 E              A S D F
//   A 0 B F              Z X C V
// ---------------------------------------------------------------------------
const KEY_MAP: Record<string, number> = {
  "1": 0x1, "2": 0x2, "3": 0x3, "4": 0xc,
  "q": 0x4, "w": 0x5, "e": 0x6, "r": 0xd,
  "a": 0x7, "s": 0x8, "d": 0x9, "f": 0xe,
  "z": 0xa, "x": 0x0, "c": 0xb, "v": 0xf,
};

// ---------------------------------------------------------------------------
// IBM Logo test ROM (132 bytes) — the classic "hello world" of CHIP-8.
// Draws the IBM logo on screen using only CLS, LD I, DRW, and JP.
// ---------------------------------------------------------------------------
const IBM_LOGO: number[] = [
  0x00, 0xe0, 0xa2, 0x2a, 0x60, 0x0c, 0x61, 0x08,
  0xd0, 0x1f, 0x70, 0x09, 0xa2, 0x39, 0xd0, 0x1f,
  0xa2, 0x48, 0x70, 0x08, 0xd0, 0x1f, 0x70, 0x04,
  0xa2, 0x57, 0xd0, 0x1f, 0x70, 0x08, 0xa2, 0x66,
  0xd0, 0x1f, 0x70, 0x08, 0xa2, 0x75, 0xd0, 0x1f,
  0x12, 0x28, 0xff, 0x00, 0xff, 0x00, 0x3c, 0x00,
  0x3c, 0x00, 0x3c, 0x00, 0x3c, 0x00, 0xff, 0x00,
  0xff, 0xff, 0x00, 0xff, 0x00, 0x38, 0x00, 0x3f,
  0x00, 0x3f, 0x00, 0x38, 0x00, 0xff, 0x00, 0xff,
  0x80, 0x00, 0xe0, 0x00, 0xe0, 0x00, 0x80, 0x00,
  0x80, 0x00, 0xe0, 0x00, 0xe0, 0x00, 0x80, 0xf8,
  0x00, 0xfc, 0x00, 0x3e, 0x00, 0x3f, 0x00, 0x3b,
  0x00, 0x39, 0x00, 0xf8, 0x00, 0xf8, 0x03, 0x00,
  0x07, 0x00, 0x0f, 0x00, 0xbf, 0x00, 0xfb, 0x00,
  0xf3, 0x00, 0xe3, 0x00, 0x43, 0xe0, 0x00, 0xe0,
  0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80,
  0x00, 0xe0, 0x00, 0xe0,
];

// ---------------------------------------------------------------------------
// A simple "Particle" test ROM that animates a bouncing pixel.
// Uses: CLS, LD, ADD, DRW, JP, SE, SNE — exercises more opcodes.
// ---------------------------------------------------------------------------
const PARTICLE_ROM: number[] = [
  // Setup: V0=x, V1=y, V2=dx(1), V3=dy(1), V4=sprite byte
  0x00, 0xe0, // CLS
  0x60, 0x20, // LD V0, 32  (x start = center)
  0x61, 0x10, // LD V1, 16  (y start = center)
  0x62, 0x01, // LD V2, 1   (dx = +1)
  0x63, 0x01, // LD V3, 1   (dy = +1)
  0xa2, 0x2c, // LD I, 0x22C (sprite data at end)

  // Main loop (address 0x20C):
  0x00, 0xe0, // CLS
  0xd0, 0x11, // DRW V0, V1, 1  (draw 1-row sprite)
  0x80, 0x24, // ADD V0, V2     (x += dx)
  0x81, 0x34, // ADD V1, V3     (y += dy)

  // Bounce X: if V0 == 63, flip dx
  0x64, 0x3f, // LD V4, 63
  0x50, 0x40, // SE V0, V4
  0x12, 0x1e, // JP skip_flip_x (0x21E)
  0x72, 0xff, // ADD V2, 0xFF  (dx = dx - 1, wrapping: 1->0)
  0x72, 0xff, // ADD V2, 0xFF  (dx = 0 - 1 = 0xFF = -1)
  // if V0 == 0, flip dx
  0x12, 0x22, // JP skip_check_x0 (0x222)
  0x30, 0x00, // SE V0, 0 — skip_flip_x label (0x21E)
  0x12, 0x22, // JP skip_check_x0 (0x222)
  0x62, 0x01, // LD V2, 1  (dx = +1)

  // skip_check_x0 (0x222):
  // Bounce Y: if V1 == 31, flip dy
  0x64, 0x1f, // LD V4, 31
  0x51, 0x40, // SE V1, V4
  0x12, 0x2a, // JP skip_flip_y
  0x63, 0xff, // LD V3, 0xFF (dy = -1)
  0x12, 0x0c, // JP main_loop (0x20C)

  // skip_flip_y (0x22A):
  0x12, 0x0c, // JP main_loop (0x20C)

  // Sprite data at 0x22C:
  0x80, // 1 pixel (top-left of the 8-wide sprite)
];

// ---------------------------------------------------------------------------
// Keypad test ROM — displays pressed key value on screen.
// Exercises: FX0A (wait for key), FX29 (font sprite), DRW, JP
// ---------------------------------------------------------------------------
const KEYPAD_TEST_ROM: number[] = [
  // Loop start (0x200):
  0x00, 0xe0, // CLS
  0xf0, 0x0a, // LD V0, K   (wait for keypress)
  0xf0, 0x29, // LD F, V0   (I = font sprite for pressed key)
  0x61, 0x0c, // LD V1, 12  (x = 12, roughly centered)
  0x62, 0x0a, // LD V2, 10  (y = 10)
  0xd1, 0x25, // DRW V1, V2, 5  (draw 5-row font char)
  0x12, 0x00, // JP 0x200   (loop back)
];

type RomEntry = { name: string; bytes: number[]; description: string };

const ROMS: RomEntry[] = [
  { name: "IBM Logo", bytes: IBM_LOGO, description: "Classic test — draws IBM logo" },
  { name: "Particle", bytes: PARTICLE_ROM, description: "Bouncing pixel animation" },
  { name: "Keypad Test", bytes: KEYPAD_TEST_ROM, description: "Press keys to see hex values" },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------
export default function Chip8Console() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const cpuRef = useRef<any>(null);
  const wasmMemRef = useRef<WebAssembly.Memory | null>(null);
  const rafRef = useRef<number>(0);
  const cpuIntervalRef = useRef<number>(0);
  const timerIntervalRef = useRef<number>(0);

  const [status, setStatus] = useState<"loading" | "ready" | "running" | "error">("loading");
  const [error, setError] = useState<string | null>(null);
  const [selectedRom, setSelectedRom] = useState(0);

  // -----------------------------------------------------------------------
  // Initialise WASM
  // -----------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const wasm = await import("@chip8_core");
        const exports = await wasm.default();

        if (cancelled) return;

        wasmMemRef.current = exports.memory;
        const cpu = new wasm.Cpu();
        cpuRef.current = cpu;

        setStatus("ready");
      } catch (err: any) {
        if (!cancelled) {
          setError(err.message ?? String(err));
          setStatus("error");
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  // -----------------------------------------------------------------------
  // Render the display buffer to canvas
  // -----------------------------------------------------------------------
  const renderDisplay = useCallback(() => {
    const canvas = canvasRef.current;
    const cpu = cpuRef.current;
    const mem = wasmMemRef.current;
    if (!canvas || !cpu || !mem) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const ptr = cpu.display_ptr();
    const len = cpu.display_len();
    const display = new Uint8Array(mem.buffer, ptr, len);

    const imageData = ctx.createImageData(CHIP8_W, CHIP8_H);
    const pixels = imageData.data;

    for (let i = 0; i < len; i++) {
      const on = display[i] !== 0;
      const p = i * 4;
      pixels[p] = on ? FG[0] : BG[0];
      pixels[p + 1] = on ? FG[1] : BG[1];
      pixels[p + 2] = on ? FG[2] : BG[2];
      pixels[p + 3] = 0xff;
    }

    ctx.putImageData(imageData, 0, 0);
  }, []);

  // -----------------------------------------------------------------------
  // Load & run a ROM
  // -----------------------------------------------------------------------
  const loadAndRun = useCallback((romBytes: number[]) => {
    const cpu = cpuRef.current;
    if (!cpu) return;

    // Stop any existing loop
    cancelAnimationFrame(rafRef.current);
    clearInterval(cpuIntervalRef.current);
    clearInterval(timerIntervalRef.current);

    cpu.reset();
    cpu.load_rom(new Uint8Array(romBytes));
    setStatus("running");

    // CPU tick at ~500 Hz (batched: 8 instructions per 16ms interval)
    const TICKS_PER_FRAME = 8;
    cpuIntervalRef.current = window.setInterval(() => {
      for (let i = 0; i < TICKS_PER_FRAME; i++) {
        cpu.tick_cpu();
      }
    }, 16);

    // Timers at 60 Hz
    timerIntervalRef.current = window.setInterval(() => {
      cpu.tick_timers();
    }, 1000 / 60);

    // Render at 60 Hz via rAF
    const draw = () => {
      renderDisplay();
      rafRef.current = requestAnimationFrame(draw);
    };
    rafRef.current = requestAnimationFrame(draw);
  }, [renderDisplay]);

  // -----------------------------------------------------------------------
  // Keyboard input
  // -----------------------------------------------------------------------
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const key = KEY_MAP[e.key.toLowerCase()];
      if (key !== undefined) {
        e.preventDefault();
        cpuRef.current?.key_down(key);
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      const key = KEY_MAP[e.key.toLowerCase()];
      if (key !== undefined) {
        e.preventDefault();
        cpuRef.current?.key_up(key);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, []);

  // -----------------------------------------------------------------------
  // Cleanup on unmount
  // -----------------------------------------------------------------------
  useEffect(() => {
    return () => {
      cancelAnimationFrame(rafRef.current);
      clearInterval(cpuIntervalRef.current);
      clearInterval(timerIntervalRef.current);
      cpuRef.current?.free();
    };
  }, []);

  // -----------------------------------------------------------------------
  // On-screen keypad button handler
  // -----------------------------------------------------------------------
  const handlePadDown = useCallback((key: number) => {
    cpuRef.current?.key_down(key);
  }, []);

  const handlePadUp = useCallback((key: number) => {
    cpuRef.current?.key_up(key);
  }, []);

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------
  if (status === "error") {
    return <p className="text-red-400 text-sm">Failed to load CHIP-8 WASM: {error}</p>;
  }

  if (status === "loading") {
    return <p className="text-zinc-400 text-sm animate-pulse">Loading CHIP-8 emulator...</p>;
  }

  const KEYPAD_LAYOUT = [
    [0x1, 0x2, 0x3, 0xc],
    [0x4, 0x5, 0x6, 0xd],
    [0x7, 0x8, 0x9, 0xe],
    [0xa, 0x0, 0xb, 0xf],
  ];

  const KEY_LABELS: Record<number, string> = {
    0x0: "0", 0x1: "1", 0x2: "2", 0x3: "3",
    0x4: "4", 0x5: "5", 0x6: "6", 0x7: "7",
    0x8: "8", 0x9: "9", 0xa: "A", 0xb: "B",
    0xc: "C", 0xd: "D", 0xe: "E", 0xf: "F",
  };

  // Keyboard hint labels for the on-screen pad
  const KB_HINTS: Record<number, string> = {
    0x1: "1", 0x2: "2", 0x3: "3", 0xc: "4",
    0x4: "Q", 0x5: "W", 0x6: "E", 0xd: "R",
    0x7: "A", 0x8: "S", 0x9: "D", 0xe: "F",
    0xa: "Z", 0x0: "X", 0xb: "C", 0xf: "V",
  };

  return (
    <div className="space-y-4">
      {/* ROM selector & run button */}
      <div className="flex flex-wrap items-center gap-3">
        <select
          value={selectedRom}
          onChange={(e) => setSelectedRom(Number(e.target.value))}
          className="rounded bg-zinc-700 text-zinc-100 text-sm px-3 py-1.5 border border-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-400"
        >
          {ROMS.map((rom, i) => (
            <option key={rom.name} value={i}>
              {rom.name} — {rom.description}
            </option>
          ))}
        </select>
        <button
          onClick={() => loadAndRun(ROMS[selectedRom].bytes)}
          className="rounded bg-emerald-500 hover:bg-emerald-400 text-zinc-900 font-medium text-sm px-4 py-1.5 transition-colors"
        >
          {status === "running" ? "Restart" : "Run"}
        </button>
      </div>

      {/* Display canvas */}
      <div className="flex flex-col sm:flex-row gap-4 items-start">
        <canvas
          ref={canvasRef}
          width={CHIP8_W}
          height={CHIP8_H}
          className="rounded border border-zinc-600 bg-[#0a0f0a]"
          style={{
            width: CANVAS_W,
            height: CANVAS_H,
            imageRendering: "pixelated",
          }}
        />

        {/* On-screen keypad */}
        <div className="grid grid-cols-4 gap-1.5 shrink-0">
          {KEYPAD_LAYOUT.flat().map((key) => (
            <button
              key={key}
              onPointerDown={() => handlePadDown(key)}
              onPointerUp={() => handlePadUp(key)}
              onPointerLeave={() => handlePadUp(key)}
              className="w-11 h-11 rounded bg-zinc-700 hover:bg-zinc-600 active:bg-emerald-500 active:text-zinc-900 text-zinc-300 text-xs font-mono flex flex-col items-center justify-center leading-tight transition-colors select-none"
            >
              <span className="font-bold">{KEY_LABELS[key]}</span>
              <span className="text-[9px] text-zinc-500">{KB_HINTS[key]}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Instructions */}
      <p className="text-xs text-zinc-500">
        Keys: 1-4 / Q-R / A-F / Z-V map to CHIP-8 hex pad 0-F.
        {status === "ready" && " Select a ROM and press Run to start."}
      </p>
    </div>
  );
}
