import SnakeGame from "./components/SnakeGame";
import SandBox from "./components/SandBox";
import FractalExplorer from "./components/FractalExplorer";
import FlockSim from "./components/FlockSim";
import Chip8Console from "./components/Chip8Console";
import SynthLab from "./components/SynthLab";
import ToolCard from "./components/ToolCard";

export default function App() {
  return (
    <div className="min-h-screen bg-zinc-900 text-zinc-100">
      {/* Header */}
      <header className="sticky top-0 z-10 border-b border-zinc-800 bg-zinc-900/80 backdrop-blur">
        <div className="mx-auto max-w-5xl px-3 sm:px-4 py-3 sm:py-4 flex items-center justify-between">
          <h1 className="text-lg sm:text-xl font-bold tracking-tight">
            wiz<span className="text-emerald-400">within</span>
          </h1>
          <span className="text-xs text-zinc-500">
            Rust + WASM micro-tools
          </span>
        </div>
      </header>

      {/* Scrollable dashboard */}
      <main className="mx-auto max-w-5xl px-3 sm:px-4 py-4 sm:py-8 space-y-4 sm:space-y-6">
        <ToolCard
          title="Snake"
          description="Classic snake game — logic & rendering powered by Rust/WebGL."
        >
          <SnakeGame />
        </ToolCard>

        <ToolCard
          title="Elemental SandBox"
          description="Falling sand cellular automata — shared-memory rendering from Rust to Canvas."
        >
          <SandBox />
        </ToolCard>

        <ToolCard
          title="Fractal Explorer"
          description="Mandelbrot set — f64 escape-time computed in Rust/WASM with smooth pan & zoom."
        >
          <FractalExplorer />
        </ToolCard>

        <ToolCard
          title="Flocking Simulation"
          description="Boids algorithm — separation, alignment &amp; cohesion computed in Rust/WASM, rendered via Canvas 2D."
        >
          <FlockSim />
        </ToolCard>

        <ToolCard
          title="CHIP-8 Emulator"
          description="Classic CHIP-8 interpreter — fetch-decode-execute cycle running in Rust/WASM at 500 Hz."
        >
          <Chip8Console />
        </ToolCard>

        <ToolCard
          title="Sonic Alchemy"
          description="Real-time audio synthesizer — phase-accumulator oscillator in Rust/WASM, streamed via AudioContext."
        >
          <SynthLab />
        </ToolCard>

        {/* Placeholder for future tools */}
        <div className="rounded-xl border border-dashed border-zinc-700 p-8 text-center text-zinc-600 text-sm">
          More micro-tools coming soon...
        </div>
      </main>
    </div>
  );
}
