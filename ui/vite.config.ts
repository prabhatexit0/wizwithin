import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import path from "path";

export default defineConfig({
  plugins: [react(), tailwindcss(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      "@snake_engine": path.resolve(
        __dirname,
        "../crates/snake_engine/pkg",
      ),
      "@sand_engine": path.resolve(
        __dirname,
        "../crates/sand_engine/pkg",
      ),
      "@fractal_engine": path.resolve(
        __dirname,
        "../crates/fractal_engine/pkg",
      ),
      "@boid_engine": path.resolve(
        __dirname,
        "../crates/boid_engine/pkg",
      ),
      "@chip8_core": path.resolve(
        __dirname,
        "../crates/chip8_core/pkg",
      ),
      "@synth_engine": path.resolve(
        __dirname,
        "../crates/synth_engine/pkg",
      ),
      "@raycaster_engine": path.resolve(
        __dirname,
        "../crates/raycaster_engine/pkg",
      ),
    },
  },
  build: {
    target: "esnext",
  },
  optimizeDeps: {
    exclude: ["@snake_engine", "@sand_engine", "@fractal_engine", "@boid_engine", "@chip8_core", "@synth_engine", "@raycaster_engine"],
  },
});
