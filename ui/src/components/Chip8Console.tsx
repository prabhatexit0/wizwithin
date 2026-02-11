import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// CHIP-8 display constants
// ---------------------------------------------------------------------------
const CHIP8_W = 64;
const CHIP8_H = 32;

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
// Particle bounce ROM — XOR-draw a pixel that bounces off all four edges.
//
// Uses a delay timer (2 ticks @ 60 Hz ≈ 30 fps) for visible animation and
// SNE (skip-if-not-equal) for clean boundary checks instead of JP spaghetti.
//
// Registers:  V0=x  V1=y  V2=dx  V3=dy  V5=scratch
//
// Address  Bytes   Instruction
// ------- ------  -------------------------------------------
// 0x200   00 E0   CLS
// 0x202   60 01   LD V0, 1           x = 1
// 0x204   61 01   LD V1, 1           y = 1
// 0x206   62 01   LD V2, 1           dx = +1
// 0x208   63 01   LD V3, 1           dy = +1
// 0x20A   A2 32   LD I, 0x232        sprite address
// 0x20C   D0 11   DRW V0, V1, 1      initial draw
//
// LOOP (0x20E):
// 0x20E   65 02   LD V5, 2           delay = 2 (~33 ms)
// 0x210   F5 15   LD DT, V5
// WAIT (0x212):
// 0x212   F5 07   LD V5, DT
// 0x214   35 00   SE V5, 0           skip if timer expired
// 0x216   12 12   JP 0x212           spin-wait
//
// 0x218   D0 11   DRW V0, V1, 1      erase old pixel (XOR)
// 0x21A   80 24   ADD V0, V2         x += dx
// 0x21C   81 34   ADD V1, V3         y += dy
//
// 0x21E   40 3F   SNE V0, 63         skip next if x ≠ 63
// 0x220   62 FF   LD V2, 0xFF        dx = -1
// 0x222   40 00   SNE V0, 0          skip next if x ≠ 0
// 0x224   62 01   LD V2, 1           dx = +1
//
// 0x226   41 1F   SNE V1, 31         skip next if y ≠ 31
// 0x228   63 FF   LD V3, 0xFF        dy = -1
// 0x22A   41 00   SNE V1, 0          skip next if y ≠ 0
// 0x22C   63 01   LD V3, 1           dy = +1
//
// 0x22E   D0 11   DRW V0, V1, 1      draw new pixel
// 0x230   12 0E   JP 0x20E           loop
//
// 0x232   80      sprite: single pixel (MSB of 8-wide row)
// ---------------------------------------------------------------------------
const PARTICLE_ROM: number[] = [
  0x00, 0xe0,
  0x60, 0x01,
  0x61, 0x01,
  0x62, 0x01,
  0x63, 0x01,
  0xa2, 0x32,
  0xd0, 0x11,
  // LOOP
  0x65, 0x02,
  0xf5, 0x15,
  // WAIT
  0xf5, 0x07,
  0x35, 0x00,
  0x12, 0x12,
  // move
  0xd0, 0x11,
  0x80, 0x24,
  0x81, 0x34,
  // bounce X
  0x40, 0x3f,
  0x62, 0xff,
  0x40, 0x00,
  0x62, 0x01,
  // bounce Y
  0x41, 0x1f,
  0x63, 0xff,
  0x41, 0x00,
  0x63, 0x01,
  // draw + loop
  0xd0, 0x11,
  0x12, 0x0e,
  // sprite
  0x80,
];

// ---------------------------------------------------------------------------
// Keypad test ROM — displays pressed key value on screen.
//
// FX0A (wait-for-key) comes FIRST so the drawn character stays on screen
// while waiting for the next key press.
//
// 0x200   F0 0A   LD V0, K           wait for key
// 0x202   00 E0   CLS                clear old char
// 0x204   F0 29   LD F, V0           I = font for V0
// 0x206   61 1E   LD V1, 30          x ≈ center
// 0x208   62 0E   LD V2, 14          y ≈ center
// 0x20A   D1 25   DRW V1, V2, 5      draw 5-row font
// 0x20C   12 00   JP 0x200           loop
// ---------------------------------------------------------------------------
const KEYPAD_TEST_ROM: number[] = [
  0xf0, 0x0a,
  0x00, 0xe0,
  0xf0, 0x29,
  0x61, 0x1e,
  0x62, 0x0e,
  0xd1, 0x25,
  0x12, 0x00,
];

type RomEntry = { name: string; bytes: number[]; description: string };

const ROMS: RomEntry[] = [
  { name: "IBM Logo", bytes: IBM_LOGO, description: "Classic test — draws IBM logo" },
  { name: "Particle", bytes: PARTICLE_ROM, description: "Bouncing pixel animation" },
  { name: "Keypad Test", bytes: KEYPAD_TEST_ROM, description: "Press keys to see hex values" },
];

// ---------------------------------------------------------------------------
// Keypad constants (hoisted outside render)
// ---------------------------------------------------------------------------
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

const KB_HINTS: Record<number, string> = {
  0x1: "1", 0x2: "2", 0x3: "3", 0xc: "4",
  0x4: "Q", 0x5: "W", 0x6: "E", 0xd: "R",
  0x7: "A", 0x8: "S", 0x9: "D", 0xe: "F",
  0xa: "Z", 0x0: "X", 0xb: "C", 0xf: "V",
};

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

  return (
    <div className="space-y-4">
      {/* ROM selector & run button */}
      <div className="flex flex-wrap items-center gap-2 sm:gap-3">
        <select
          value={selectedRom}
          onChange={(e) => setSelectedRom(Number(e.target.value))}
          className="min-w-0 flex-1 sm:flex-none rounded bg-zinc-700 text-zinc-100 text-sm px-2 sm:px-3 py-1.5 border border-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-400"
        >
          {ROMS.map((rom, i) => (
            <option key={rom.name} value={i}>
              {rom.name} — {rom.description}
            </option>
          ))}
        </select>
        <button
          onClick={() => loadAndRun(ROMS[selectedRom].bytes)}
          className="rounded bg-emerald-500 hover:bg-emerald-400 text-zinc-900 font-medium text-sm px-4 py-1.5 transition-colors shrink-0"
        >
          {status === "running" ? "Restart" : "Run"}
        </button>
      </div>

      {/* Display + keypad */}
      <div className="flex flex-col gap-4 items-start">
        {/* Display canvas — responsive: fills parent up to 640px, keeps 2:1 ratio */}
        <canvas
          ref={canvasRef}
          width={CHIP8_W}
          height={CHIP8_H}
          className="w-full max-w-[640px] rounded border border-zinc-600 bg-[#0a0f0a]"
          style={{
            aspectRatio: "2 / 1",
            imageRendering: "pixelated",
          }}
        />

        {/* On-screen keypad — touch-friendly (48×48 targets) */}
        <div
          className="grid grid-cols-4 gap-1.5 sm:gap-2"
          style={{ touchAction: "manipulation" }}
        >
          {KEYPAD_LAYOUT.flat().map((key) => (
            <button
              key={key}
              onPointerDown={() => handlePadDown(key)}
              onPointerUp={() => handlePadUp(key)}
              onPointerLeave={() => handlePadUp(key)}
              onContextMenu={(e) => e.preventDefault()}
              className="w-12 h-12 rounded bg-zinc-700 hover:bg-zinc-600 active:bg-emerald-500 active:text-zinc-900 text-zinc-300 text-xs font-mono flex flex-col items-center justify-center leading-tight transition-colors select-none touch-manipulation"
            >
              <span className="font-bold">{KEY_LABELS[key]}</span>
              <span className="text-[9px] text-zinc-500">{KB_HINTS[key]}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Instructions */}
      <p className="text-xs text-zinc-500">
        Tap the keypad or use keyboard: 1-4 / Q-R / A-F / Z-V → CHIP-8 hex pad 0-F.
        {status === "ready" && " Select a ROM and press Run to start."}
      </p>
    </div>
  );
}
