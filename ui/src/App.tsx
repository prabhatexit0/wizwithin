import SnakeGame from "./components/SnakeGame";
import SandBox from "./components/SandBox";
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

        {/* Placeholder for future tools */}
        <div className="rounded-xl border border-dashed border-zinc-700 p-8 text-center text-zinc-600 text-sm">
          More micro-tools coming soon...
        </div>
      </main>
    </div>
  );
}
