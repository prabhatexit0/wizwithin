import { useState, useEffect, useCallback, type ComponentType } from "react";
import SnakeGame from "./components/SnakeGame";
import SandBox from "./components/SandBox";
import FractalExplorer from "./components/FractalExplorer";
import FlockSim from "./components/FlockSim";
import Chip8Console from "./components/Chip8Console";
import SynthLab from "./components/SynthLab";
import Raycaster from "./components/Raycaster";
import EvolutionLab from "./components/EvolutionLab";

interface MicroApp {
  slug: string;
  title: string;
  description: string;
  component: ComponentType;
  color: string;
}

const APPS: MicroApp[] = [
  {
    slug: "snake",
    title: "Snake",
    description:
      "Classic snake game — logic & rendering powered by Rust/WebGL.",
    component: SnakeGame,
    color: "#34d399",
  },
  {
    slug: "sandbox",
    title: "Elemental SandBox",
    description:
      "Falling sand cellular automata — shared-memory rendering from Rust to Canvas.",
    component: SandBox,
    color: "#fbbf24",
  },
  {
    slug: "fractal",
    title: "Fractal Explorer",
    description:
      "Mandelbrot set — f64 escape-time computed in Rust/WASM with smooth pan & zoom.",
    component: FractalExplorer,
    color: "#a78bfa",
  },
  {
    slug: "flock",
    title: "Flocking Simulation",
    description:
      "Boids algorithm — separation, alignment & cohesion computed in Rust/WASM, rendered via Canvas 2D.",
    component: FlockSim,
    color: "#38bdf8",
  },
  {
    slug: "chip8",
    title: "CHIP-8 Emulator",
    description:
      "Classic CHIP-8 interpreter — fetch-decode-execute cycle running in Rust/WASM at 500 Hz.",
    component: Chip8Console,
    color: "#fb7185",
  },
  {
    slug: "synth",
    title: "Sonic Alchemy",
    description:
      "Real-time audio synthesizer — phase-accumulator oscillator in Rust/WASM, streamed via AudioContext.",
    component: SynthLab,
    color: "#fb923c",
  },
  {
    slug: "raycaster",
    title: "Retro Raycaster",
    description:
      "Wolfenstein-style 2.5D maze explorer — DDA raycasting in Rust/WASM with psychedelic procedural walls.",
    component: Raycaster,
    color: "#e879f9",
  },
  {
    slug: "evolution",
    title: "Darwin's Blobs",
    description:
      "Evolutionary soft-body simulation — 100 spring creatures learn to walk via genetic algorithm in Rust/WASM.",
    component: EvolutionLab,
    color: "#86efac",
  },
];

function getAppFromUrl(): string | null {
  const params = new URLSearchParams(window.location.search);
  return params.get("app");
}

export default function App() {
  const [activeSlug, setActiveSlug] = useState<string | null>(getAppFromUrl);
  const [toastVisible, setToastVisible] = useState(false);

  // Sync with browser back/forward
  useEffect(() => {
    const handler = () => setActiveSlug(getAppFromUrl());
    window.addEventListener("popstate", handler);
    return () => window.removeEventListener("popstate", handler);
  }, []);

  const navigate = useCallback((slug: string | null) => {
    const url = slug
      ? `${window.location.pathname}?app=${slug}`
      : window.location.pathname;
    window.history.pushState({}, "", url);
    setActiveSlug(slug);
    window.scrollTo({ top: 0, behavior: "smooth" });
  }, []);

  // Escape key exits fullscreen
  useEffect(() => {
    if (!activeSlug) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") navigate(null);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [activeSlug, navigate]);

  // Update document title
  useEffect(() => {
    const app = activeSlug ? APPS.find((a) => a.slug === activeSlug) : null;
    document.title = app ? `${app.title} — wizwithin` : "wizwithin";
  }, [activeSlug]);

  const share = useCallback((slug: string) => {
    const url = `${window.location.origin}${window.location.pathname}?app=${slug}`;
    navigator.clipboard.writeText(url).then(() => {
      setToastVisible(true);
      setTimeout(() => setToastVisible(false), 2000);
    });
  }, []);

  const activeApp = activeSlug
    ? APPS.find((a) => a.slug === activeSlug) ?? null
    : null;

  return (
    <div className="min-h-screen bg-zinc-900 text-zinc-100">
      {/* ── Header ── */}
      <header className="sticky top-0 z-10 border-b border-zinc-800 bg-zinc-900/80 backdrop-blur">
        <div className="mx-auto max-w-7xl px-4 py-3 flex items-center gap-3">
          <button
            onClick={() => navigate(null)}
            className="text-lg font-bold tracking-tight hover:opacity-80 transition-opacity cursor-pointer"
          >
            wiz<span className="text-emerald-400">within</span>
          </button>

          {activeApp && (
            <>
              <svg
                className="w-4 h-4 text-zinc-600"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={1.5}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M9 5l7 7-7 7"
                />
              </svg>
              <span
                className="text-sm font-medium"
                style={{ color: activeApp.color }}
              >
                {activeApp.title}
              </span>
            </>
          )}

          <div className="ml-auto flex items-center gap-3">
            {activeApp && (
              <button
                onClick={() => share(activeApp.slug)}
                className="inline-flex items-center gap-1.5 text-xs text-zinc-400 hover:text-zinc-200 transition-colors cursor-pointer"
              >
                <svg
                  className="w-3.5 h-3.5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth={2}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
                  />
                </svg>
                Share
              </button>
            )}
            <span className="text-xs text-zinc-500 hidden sm:inline">
              Rust + WASM micro-tools
            </span>
          </div>
        </div>
      </header>

      {activeApp ? (
        /* ── Fullscreen single-app view ── */
        <main className="mx-auto max-w-7xl px-4 py-6">
          <button
            onClick={() => navigate(null)}
            className="inline-flex items-center gap-1.5 text-sm text-zinc-400 hover:text-zinc-200 mb-5 transition-colors group cursor-pointer"
          >
            <svg
              className="w-4 h-4 transition-transform group-hover:-translate-x-0.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M15 19l-7-7 7-7"
              />
            </svg>
            All tools
          </button>

          <div className="mb-5">
            <h2 className="text-xl font-semibold mb-1" style={{ color: activeApp.color }}>
              {activeApp.title}
            </h2>
            <p className="text-sm text-zinc-400">{activeApp.description}</p>
          </div>

          <div className="rounded-xl border border-zinc-700 bg-zinc-800/60 p-4 sm:p-6 shadow-lg">
            <activeApp.component />
          </div>

          <p className="text-center text-xs text-zinc-600 mt-4">
            Press <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-zinc-400 font-mono text-[10px]">Esc</kbd> to return
          </p>
        </main>
      ) : (
        /* ── Dashboard grid view ── */
        <main className="mx-auto max-w-6xl px-4 py-8 sm:py-12">
          <div className="mb-8 sm:mb-10 text-center">
            <h2 className="text-2xl sm:text-3xl font-bold tracking-tight mb-2">
              Micro-tools
            </h2>
            <p className="text-sm text-zinc-400 max-w-md mx-auto leading-relaxed">
              Interactive experiments built with Rust and WebAssembly.
              Click any tool to explore it.
            </p>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {APPS.map((app) => (
              <AppCard
                key={app.slug}
                app={app}
                onOpen={navigate}
                onShare={share}
              />
            ))}
          </div>

          <div className="mt-8 rounded-xl border border-dashed border-zinc-700/60 p-6 text-center text-zinc-600 text-sm">
            More micro-tools coming soon...
          </div>
        </main>
      )}

      {/* ── Footer ── */}
      {!activeApp && (
        <footer className="border-t border-zinc-800/50 mt-4">
          <div className="mx-auto max-w-6xl px-4 py-6 text-center text-xs text-zinc-600">
            Built with Rust, WebAssembly & React
          </div>
        </footer>
      )}

      {/* ── Copy toast ── */}
      <div
        className={`fixed bottom-6 left-1/2 -translate-x-1/2 px-4 py-2.5 rounded-lg bg-zinc-100 text-zinc-900 text-sm font-medium shadow-lg transition-all duration-300 pointer-events-none ${
          toastVisible
            ? "opacity-100 translate-y-0"
            : "opacity-0 translate-y-2"
        }`}
      >
        Link copied to clipboard
      </div>
    </div>
  );
}

/* ── Dashboard card ── */

function AppCard({
  app,
  onOpen,
  onShare,
}: {
  app: MicroApp;
  onOpen: (slug: string) => void;
  onShare: (slug: string) => void;
}) {
  return (
    <button
      onClick={() => onOpen(app.slug)}
      className="group text-left rounded-xl border border-zinc-700/80 bg-zinc-800/50 p-5 shadow-md hover:shadow-xl hover:border-zinc-600 hover:-translate-y-0.5 transition-all duration-200 cursor-pointer"
    >
      <div
        className="h-1 w-10 rounded-full mb-4 transition-all duration-200 group-hover:w-14"
        style={{ backgroundColor: app.color }}
      />
      <h3 className="text-base font-semibold text-zinc-100 mb-1 group-hover:text-white transition-colors">
        {app.title}
      </h3>
      <p className="text-sm text-zinc-400 leading-relaxed line-clamp-2">
        {app.description}
      </p>
      <div className="mt-4 flex items-center justify-between">
        <span
          className="text-xs font-medium transition-colors"
          style={{ color: app.color }}
        >
          Open &rarr;
        </span>
        <span
          role="button"
          tabIndex={0}
          onClick={(e) => {
            e.stopPropagation();
            onShare(app.slug);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.stopPropagation();
              onShare(app.slug);
            }
          }}
          className="text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
        >
          Share
        </span>
      </div>
    </button>
  );
}
