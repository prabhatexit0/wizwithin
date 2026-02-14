import { useEffect, useRef, useState, useCallback } from "react";

// ---------------------------------------------------------------------------
// Constants — must match Rust
// ---------------------------------------------------------------------------
const SIM_W = 640;
const SIM_H = 400;

// Entity kinds
const ENT_GRENADE = 0;
const ENT_MOLOTOV = 1;
const ENT_C4 = 2;
const ENT_MISSILE = 3;
const ENT_CLUSTER = 4;
const ENT_DYNAMITE = 5;
const ENT_NAPALM = 6;
const ENT_NUKE = 7;
const ENT_MIRV = 8;
const ENT_BUNKER_BUSTER = 9;
const ENT_FLASHBANG = 10;
const ENT_SHRAPNEL = 11;
const ENT_INCENDIARY = 12;
const ENT_THERMOBARIC = 13;
const ENT_LAUNCHER = 14;

// Event kinds
const EVT_EXPLOSION = 0;
const EVT_BOUNCE = 1;
const EVT_FIRE_IGNITED = 2;
const EVT_MISSILE_LAUNCH = 3;
const EVT_FUSE_TICK = 4;

// Prefab kinds
const PREFAB_HOUSE = 0;
const PREFAB_TREE = 1;
const PREFAB_POND = 2;
const PREFAB_BUNKER = 3;
const PREFAB_TOWER = 4;
const PREFAB_BRIDGE = 5;
const PREFAB_WALL = 6;
const PREFAB_GROUND = 7;
const PREFAB_VILLAGE = 8;
const PREFAB_FORTRESS = 9;

type ToolMode = "build" | "chaos";

type BuildTool = "house" | "tree" | "pond" | "bunker" | "tower" | "bridge" | "wall" | "ground" | "village" | "fortress";
type ChaosTool =
  | "grenade" | "molotov" | "missile" | "c4" | "cluster" | "dynamite" | "napalm"
  | "nuke" | "mirv" | "bunker_buster" | "flashbang" | "shrapnel" | "incendiary"
  | "thermobaric" | "launcher";

const BUILD_TOOLS: { id: BuildTool; label: string; prefab: number; color: string; desc: string }[] = [
  { id: "ground", label: "Ground", prefab: PREFAB_GROUND, color: "#6b7280", desc: "Rolling terrain with grass, dirt, and bedrock" },
  { id: "village", label: "Village", prefab: PREFAB_VILLAGE, color: "#92400e", desc: "Complete scene: houses, trees, and pond on terrain" },
  { id: "fortress", label: "Fortress", prefab: PREFAB_FORTRESS, color: "#78716c", desc: "Complete scene: tower, walls, and bunker on terrain" },
  { id: "house", label: "House", prefab: PREFAB_HOUSE, color: "#8b5a2b", desc: "Wood walls, stone roof, glass windows" },
  { id: "tree", label: "Tree", prefab: PREFAB_TREE, color: "#4a9e3f", desc: "Wooden trunk with leafy canopy" },
  { id: "pond", label: "Pond", prefab: PREFAB_POND, color: "#3b82f6", desc: "Water pond with dirt rim" },
  { id: "bunker", label: "Bunker", prefab: PREFAB_BUNKER, color: "#94a3b8", desc: "Steel-reinforced shelter" },
  { id: "tower", label: "Tower", prefab: PREFAB_TOWER, color: "#a8a29e", desc: "Tall stone watchtower" },
  { id: "bridge", label: "Bridge", prefab: PREFAB_BRIDGE, color: "#92400e", desc: "Wooden bridge with stone supports" },
  { id: "wall", label: "Wall", prefab: PREFAB_WALL, color: "#78716c", desc: "Stone fortification wall" },
];

interface ChaosToolDef {
  id: ChaosTool;
  label: string;
  kind: number;
  color: string;
  desc: string;
  throwable: boolean;
  category: "throwable" | "placed" | "strike" | "launcher";
}

const CHAOS_TOOLS: ChaosToolDef[] = [
  // Throwables — drag to aim
  { id: "grenade", label: "Grenade", kind: ENT_GRENADE, color: "#4ade80", desc: "Bounces, 3s fuse", throwable: true, category: "throwable" },
  { id: "molotov", label: "Molotov", kind: ENT_MOLOTOV, color: "#f97316", desc: "Shatters into fire on impact", throwable: true, category: "throwable" },
  { id: "cluster", label: "Cluster", kind: ENT_CLUSTER, color: "#06b6d4", desc: "Splits into 8 explosions", throwable: true, category: "throwable" },
  { id: "napalm", label: "Napalm", kind: ENT_NAPALM, color: "#f59e0b", desc: "Massive fire zone on impact", throwable: true, category: "throwable" },
  { id: "flashbang", label: "Flash", kind: ENT_FLASHBANG, color: "#e5e7eb", desc: "Huge white shockwave, little damage", throwable: true, category: "throwable" },
  { id: "shrapnel", label: "Shrapnel", kind: ENT_SHRAPNEL, color: "#b91c1c", desc: "40 random penetrating rays", throwable: true, category: "throwable" },
  { id: "mirv", label: "MIRV", kind: ENT_MIRV, color: "#14b8a6", desc: "Splits into 5 grenades", throwable: true, category: "throwable" },
  { id: "nuke", label: "Nuke", kind: ENT_NUKE, color: "#fef08a", desc: "Enormous blast + fire ring", throwable: true, category: "throwable" },
  { id: "thermobaric", label: "Thermo", kind: ENT_THERMOBARIC, color: "#dc2626", desc: "Massive blast + pressure wave + fire", throwable: true, category: "throwable" },
  // Placed — click to drop
  { id: "dynamite", label: "Dynamite", kind: ENT_DYNAMITE, color: "#ef4444", desc: "Place, auto-detonates 1.5s", throwable: false, category: "placed" },
  { id: "c4", label: "C4", kind: ENT_C4, color: "#f87171", desc: "Place, then detonate all", throwable: false, category: "placed" },
  { id: "incendiary", label: "Incendy", kind: ENT_INCENDIARY, color: "#fb923c", desc: "Place, 3s fuse, fire blanket", throwable: false, category: "placed" },
  // Strikes — click target
  { id: "missile", label: "Missile", kind: ENT_MISSILE, color: "#fbbf24", desc: "Fires from launchers or screen edge", throwable: false, category: "strike" },
  { id: "bunker_buster", label: "Buster", kind: ENT_BUNKER_BUSTER, color: "#9ca3af", desc: "Drops from sky, penetrates terrain", throwable: false, category: "strike" },
  // Launcher
  { id: "launcher", label: "Launcher", kind: ENT_LAUNCHER, color: "#22c55e", desc: "Place launchers, then use Missile to fire from them", throwable: false, category: "launcher" },
];

// ---------------------------------------------------------------------------
// Procedural Web Audio — event-driven 8-bit sounds
// ---------------------------------------------------------------------------

let audioCtx: AudioContext | null = null;

function getAudioCtx(): AudioContext {
  if (!audioCtx) audioCtx = new AudioContext();
  if (audioCtx.state === "suspended") audioCtx.resume();
  return audioCtx;
}

function playExplosion(power: number) {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;
  const vol = Math.min(1, power / 150) * 0.4;

  const osc = ctx.createOscillator();
  osc.type = "square";
  osc.frequency.setValueAtTime(90, now);
  osc.frequency.exponentialRampToValueAtTime(25, now + 0.5);
  const g = ctx.createGain();
  g.gain.setValueAtTime(vol, now);
  g.gain.exponentialRampToValueAtTime(0.001, now + 0.6);
  osc.connect(g).connect(ctx.destination);
  osc.start(now);
  osc.stop(now + 0.6);

  const nLen = Math.floor(ctx.sampleRate * 0.15);
  const nBuf = ctx.createBuffer(1, nLen, ctx.sampleRate);
  const nd = nBuf.getChannelData(0);
  for (let i = 0; i < nLen; i++) nd[i] = (Math.random() * 2 - 1) * Math.exp(-i / (ctx.sampleRate * 0.03));
  const ns = ctx.createBufferSource();
  ns.buffer = nBuf;
  const lp = ctx.createBiquadFilter();
  lp.type = "lowpass";
  lp.frequency.setValueAtTime(800, now);
  lp.frequency.exponentialRampToValueAtTime(60, now + 0.2);
  const ng = ctx.createGain();
  ng.gain.setValueAtTime(vol * 0.7, now);
  ng.gain.exponentialRampToValueAtTime(0.001, now + 0.25);
  ns.connect(lp).connect(ng).connect(ctx.destination);
  ns.start(now);
}

function playBounce(velocity: number) {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;
  const freq = 600 + velocity * 80;
  const vol = Math.min(0.15, velocity * 0.03);

  const osc = ctx.createOscillator();
  osc.type = "sine";
  osc.frequency.setValueAtTime(freq, now);
  osc.frequency.exponentialRampToValueAtTime(freq * 0.5, now + 0.06);
  const g = ctx.createGain();
  g.gain.setValueAtTime(vol, now);
  g.gain.exponentialRampToValueAtTime(0.001, now + 0.08);
  osc.connect(g).connect(ctx.destination);
  osc.start(now);
  osc.stop(now + 0.08);
}

function playFireIgnited() {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;

  const nLen = Math.floor(ctx.sampleRate * 0.5);
  const nBuf = ctx.createBuffer(1, nLen, ctx.sampleRate);
  const nd = nBuf.getChannelData(0);
  for (let i = 0; i < nLen; i++) nd[i] = (Math.random() * 2 - 1) * Math.exp(-i / (ctx.sampleRate * 0.15));
  const ns = ctx.createBufferSource();
  ns.buffer = nBuf;
  const bp = ctx.createBiquadFilter();
  bp.type = "bandpass";
  bp.frequency.setValueAtTime(2000, now);
  bp.Q.setValueAtTime(1, now);
  const ng = ctx.createGain();
  ng.gain.setValueAtTime(0.15, now);
  ng.gain.exponentialRampToValueAtTime(0.001, now + 0.5);
  ns.connect(bp).connect(ng).connect(ctx.destination);
  ns.start(now);
}

function playMissileLaunch() {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;

  const osc = ctx.createOscillator();
  osc.type = "triangle";
  osc.frequency.setValueAtTime(200, now);
  osc.frequency.exponentialRampToValueAtTime(1200, now + 0.12);
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.2, now);
  g.gain.exponentialRampToValueAtTime(0.001, now + 0.2);
  osc.connect(g).connect(ctx.destination);
  osc.start(now);
  osc.stop(now + 0.2);
}

function playFuseTick() {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;

  const osc = ctx.createOscillator();
  osc.type = "square";
  osc.frequency.setValueAtTime(1200, now);
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.06, now);
  g.gain.exponentialRampToValueAtTime(0.001, now + 0.04);
  osc.connect(g).connect(ctx.destination);
  osc.start(now);
  osc.stop(now + 0.04);
}

function playPlaceSound() {
  const ctx = getAudioCtx();
  const now = ctx.currentTime;
  const osc = ctx.createOscillator();
  osc.type = "sine";
  osc.frequency.setValueAtTime(300, now);
  osc.frequency.exponentialRampToValueAtTime(150, now + 0.08);
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.08, now);
  g.gain.exponentialRampToValueAtTime(0.001, now + 0.1);
  osc.connect(g).connect(ctx.destination);
  osc.start(now);
  osc.stop(now + 0.1);
}

// ---------------------------------------------------------------------------
// Parse event queue from WASM memory
// ---------------------------------------------------------------------------
function processEvents(memory: WebAssembly.Memory, eventsPtr: number, eventsByteLen: number) {
  if (eventsByteLen === 0) return;

  const view = new DataView(memory.buffer, eventsPtr, eventsByteLen);
  const eventSize = 16;
  const count = eventsByteLen / eventSize;

  for (let i = 0; i < count; i++) {
    const offset = i * eventSize;
    const kind = view.getUint8(offset);
    const power = view.getFloat32(offset + 12, true);

    switch (kind) {
      case EVT_EXPLOSION:
        playExplosion(power);
        break;
      case EVT_BOUNCE:
        playBounce(power);
        break;
      case EVT_FIRE_IGNITED:
        playFireIgnited();
        break;
      case EVT_MISSILE_LAUNCH:
        playMissileLaunch();
        break;
      case EVT_FUSE_TICK:
        playFuseTick();
        break;
    }
  }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function BlastLab() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "running" | "error">("loading");
  const [errorMsg, setErrorMsg] = useState("");

  const [mode, setMode] = useState<ToolMode>("build");
  const [buildTool, setBuildTool] = useState<BuildTool>("village");
  const [chaosTool, setChaosTool] = useState<ChaosTool>("grenade");
  const [entityCount, setEntityCount] = useState(0);
  const [hasC4, setHasC4] = useState(false);
  const [hasLaunchers, setHasLaunchers] = useState(false);

  // Drag-to-aim state
  const [aiming, setAiming] = useState(false);
  const aimStart = useRef<[number, number] | null>(null);
  const aimEnd = useRef<[number, number] | null>(null);

  // Refs for RAF loop
  const simRef = useRef<InstanceType<typeof import("@blast_lab").World> | null>(null);
  const memRef = useRef<WebAssembly.Memory | null>(null);
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const buildToolRef = useRef(buildTool);
  buildToolRef.current = buildTool;
  const chaosToolRef = useRef(chaosTool);
  chaosToolRef.current = chaosTool;

  // Screen shake + flash
  const shakeRef = useRef(0);
  const flashRef = useRef(0);

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
        memRef.current = wasmMemory;
        const sim = new wasm.World();
        simRef.current = sim;

        const canvas = canvasRef.current!;
        const ctx = canvas.getContext("2d")!;
        canvas.width = SIM_W;
        canvas.height = SIM_H;

        setStatus("running");

        let frameCount = 0;

        function frame() {
          if (cancelled) return;

          sim.tick();
          sim.render();

          // Process event queue for audio
          const evtPtr = sim.events_ptr();
          const evtByteLen = sim.events_byte_len();
          if (evtByteLen > 0) {
            processEvents(wasmMemory, evtPtr, evtByteLen);

            const view = new DataView(wasmMemory.buffer, evtPtr, evtByteLen);
            for (let i = 0; i < evtByteLen / 16; i++) {
              const kind = view.getUint8(i * 16);
              if (kind === EVT_EXPLOSION) {
                const power = view.getFloat32(i * 16 + 12, true);
                shakeRef.current = Math.min(15, power / 10);
                flashRef.current = Math.min(0.8, power / 200);
              }
            }

            sim.clear_events();
          }

          // Render to canvas
          const ptr = sim.pixels_ptr();
          const len = sim.pixels_len();
          const pixels = new Uint8ClampedArray(wasmMemory.buffer, ptr, len);
          const imageData = new ImageData(pixels, SIM_W, SIM_H);
          ctx.putImageData(imageData, 0, 0);

          // Draw trajectory line
          if (aimStart.current && aimEnd.current) {
            const [sx, sy] = aimStart.current;
            const [ex, ey] = aimEnd.current;
            const dx = sx - ex;
            const dy = sy - ey;
            const throwPower = 0.08;

            ctx.strokeStyle = "rgba(255,255,255,0.6)";
            ctx.lineWidth = 1;
            ctx.setLineDash([4, 4]);
            ctx.beginPath();
            ctx.moveTo(sx, sy);

            let px = sx, py = sy;
            let vx = dx * throwPower, vy = dy * throwPower;
            for (let step = 0; step < 40; step++) {
              vx *= 0.99;
              vy += 0.18;
              px += vx;
              py += vy;
              if (px < 0 || px >= SIM_W || py < 0 || py >= SIM_H) break;
              ctx.lineTo(px, py);
            }
            ctx.stroke();
            ctx.setLineDash([]);
          }

          // Flash overlay
          if (flashRef.current > 0.01) {
            ctx.globalAlpha = flashRef.current;
            ctx.fillStyle = "rgb(255,220,150)";
            ctx.fillRect(0, 0, SIM_W, SIM_H);
            ctx.globalAlpha = 1;
            flashRef.current *= 0.88;
          }

          // Screen shake
          if (shakeRef.current > 0.5) {
            const sdx = (Math.random() - 0.5) * shakeRef.current;
            const sdy = (Math.random() - 0.5) * shakeRef.current;
            canvas.style.transform = `translate(${sdx}px, ${sdy}px)`;
            shakeRef.current *= 0.90;
          } else if (shakeRef.current > 0) {
            canvas.style.transform = "";
            shakeRef.current = 0;
          }

          // Update HUD
          frameCount++;
          if (frameCount % 8 === 0) {
            setEntityCount(sim.entity_count());
            const data = sim.get_entity_data();
            let c4Found = false;
            let launcherFound = false;
            for (let i = 0; i < data.length; i += 6) {
              if (data[i] === ENT_C4) c4Found = true;
              if (data[i] === ENT_LAUNCHER) launcherFound = true;
            }
            setHasC4(c4Found);
            setHasLaunchers(launcherFound);
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
  // Build mode: place prefab
  // -------------------------------------------------------------------------
  const placePrefab = useCallback(
    (clientX: number, clientY: number) => {
      const sim = simRef.current;
      const pt = toSim(clientX, clientY);
      if (!sim || !pt) return;
      const tool = BUILD_TOOLS.find((t) => t.id === buildToolRef.current);
      if (!tool) return;
      sim.stamp_prefab(pt[0], pt[1], tool.prefab);
      playPlaceSound();
    },
    [toSim],
  );

  // -------------------------------------------------------------------------
  // Chaos mode: spawn entity
  // -------------------------------------------------------------------------
  const spawnWithAim = useCallback(
    (startX: number, startY: number, endX: number, endY: number) => {
      const sim = simRef.current;
      if (!sim) return;
      const tool = CHAOS_TOOLS.find((t) => t.id === chaosToolRef.current);
      if (!tool) return;

      if (tool.throwable) {
        const throwPower = 0.08;
        const vx = (startX - endX) * throwPower;
        const vy = (startY - endY) * throwPower;
        sim.spawn_entity(tool.kind, startX, startY, vx, vy);
      }
    },
    [],
  );

  const spawnDirect = useCallback(
    (clientX: number, clientY: number) => {
      const sim = simRef.current;
      const pt = toSim(clientX, clientY);
      if (!sim || !pt) return;
      const tool = CHAOS_TOOLS.find((t) => t.id === chaosToolRef.current);
      if (!tool) return;

      if (tool.category === "placed" || tool.category === "launcher") {
        // Place at click point
        sim.spawn_entity(tool.kind, pt[0], pt[1], 0, 0);
        playPlaceSound();
      } else if (tool.id === "missile") {
        // If launchers exist, fire from launchers toward target
        const data = sim.get_entity_data();
        let hasLauncher = false;
        for (let i = 0; i < data.length; i += 6) {
          if (data[i] === ENT_LAUNCHER) { hasLauncher = true; break; }
        }

        if (hasLauncher) {
          sim.fire_launchers(pt[0], pt[1]);
        } else {
          // Fallback: fire from farthest screen edge
          const distTop = pt[1];
          const distBot = SIM_H - pt[1];
          const distLeft = pt[0];
          const distRight = SIM_W - pt[0];
          const maxDist = Math.max(distTop, distBot, distLeft, distRight);

          let ox: number, oy: number;
          if (maxDist === distBot) {
            ox = SIM_W / 2; oy = SIM_H + 5;
          } else if (maxDist === distTop) {
            ox = SIM_W / 2; oy = -5;
          } else if (maxDist === distLeft) {
            ox = -5; oy = SIM_H / 2;
          } else {
            ox = SIM_W + 5; oy = SIM_H / 2;
          }

          const dx = pt[0] - ox;
          const dy = pt[1] - oy;
          const dist = Math.sqrt(dx * dx + dy * dy);
          if (dist < 1) return;
          const speed = 4;
          sim.spawn_entity(ENT_MISSILE, ox, oy, (dx / dist) * speed, (dy / dist) * speed);
        }
      } else if (tool.id === "bunker_buster") {
        // Drop from directly above the click point
        sim.spawn_entity(ENT_BUNKER_BUSTER, pt[0], -10, 0, 3.0);
      }
    },
    [toSim],
  );

  // -------------------------------------------------------------------------
  // Pointer events
  // -------------------------------------------------------------------------
  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (e.button === 2) return;
      (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);
      const pt = toSim(e.clientX, e.clientY);
      if (!pt) return;

      if (modeRef.current === "build") {
        placePrefab(e.clientX, e.clientY);
      } else {
        const tool = CHAOS_TOOLS.find((t) => t.id === chaosToolRef.current);
        if (tool?.throwable) {
          setAiming(true);
          aimStart.current = pt;
          aimEnd.current = pt;
        } else {
          spawnDirect(e.clientX, e.clientY);
        }
      }
    },
    [placePrefab, spawnDirect, toSim],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!aimStart.current) return;
      const pt = toSim(e.clientX, e.clientY);
      if (pt) aimEnd.current = pt;
    },
    [toSim],
  );

  const onPointerUp = useCallback(
    (_e: React.PointerEvent<HTMLCanvasElement>) => {
      if (aimStart.current && aimEnd.current) {
        const [sx, sy] = aimStart.current;
        const [ex, ey] = aimEnd.current;
        const dragDist = Math.sqrt((sx - ex) ** 2 + (sy - ey) ** 2);
        if (dragDist > 5) {
          spawnWithAim(sx, sy, ex, ey);
        }
      }
      aimStart.current = null;
      aimEnd.current = null;
      setAiming(false);
    },
    [spawnWithAim],
  );

  const onContextMenu = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
  }, []);

  // -------------------------------------------------------------------------
  // Actions
  // -------------------------------------------------------------------------
  const handleDetonateC4 = useCallback(() => {
    simRef.current?.detonate_c4();
  }, []);

  const handleClearLaunchers = useCallback(() => {
    simRef.current?.clear_launchers();
  }, []);

  const handleReset = useCallback(() => {
    const sim = simRef.current;
    if (!sim) return;
    sim.clear();
    setEntityCount(0);
    setHasC4(false);
    setHasLaunchers(false);
    shakeRef.current = 0;
    flashRef.current = 0;
    if (canvasRef.current) canvasRef.current.style.transform = "";
  }, []);

  // -------------------------------------------------------------------------
  // Hint text
  // -------------------------------------------------------------------------
  const getHint = (): string => {
    if (mode === "build") {
      const bt = BUILD_TOOLS.find(t => t.id === buildTool);
      if (bt?.id === "village" || bt?.id === "fortress") return `Tap to generate ${bt.label} scene`;
      if (bt?.id === "ground") return "Tap to generate terrain";
      return "Tap to stamp prefab";
    }
    const ct = CHAOS_TOOLS.find(t => t.id === chaosTool);
    if (!ct) return "";
    switch (ct.category) {
      case "throwable": return "Drag to aim, release to throw";
      case "placed":
        if (ct.id === "c4") return "Tap to place C4, then Detonate";
        if (ct.id === "incendiary") return "Tap to place — 3s fuse, fire blanket";
        return "Tap to place — auto-detonates";
      case "strike":
        if (ct.id === "missile") return hasLaunchers ? "Tap target — missiles fire from launchers" : "Tap target — fires from screen edge (place Launchers first!)";
        return "Tap target — drops from sky, penetrates terrain";
      case "launcher": return "Tap to place launcher, then use Missile to fire";
    }
  };

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
          {/* Mode toggle */}
          <div className="flex flex-wrap justify-center gap-2">
            <div className="flex rounded-lg overflow-hidden border border-zinc-700">
              <button
                onClick={() => setMode("build")}
                className={`px-3 py-1.5 text-sm font-medium transition-colors cursor-pointer ${
                  mode === "build" ? "bg-emerald-600 text-white" : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                }`}
              >
                Build
              </button>
              <button
                onClick={() => setMode("chaos")}
                className={`px-3 py-1.5 text-sm font-medium transition-colors cursor-pointer ${
                  mode === "chaos" ? "bg-red-600 text-white" : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700"
                }`}
              >
                Chaos
              </button>
            </div>
          </div>

          {/* Tool buttons */}
          <div className="flex flex-wrap justify-center gap-1 max-w-[960px]">
            {mode === "build" && BUILD_TOOLS.map((tool) => (
              <button
                key={tool.id}
                onClick={() => setBuildTool(tool.id)}
                title={tool.desc}
                className={`px-2.5 py-1 text-xs rounded transition-colors cursor-pointer border ${
                  buildTool === tool.id
                    ? "text-white border-white/30"
                    : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700 border-zinc-700"
                }`}
                style={buildTool === tool.id ? { backgroundColor: tool.color } : undefined}
              >
                {tool.label}
              </button>
            ))}

            {mode === "chaos" && CHAOS_TOOLS.map((tool) => (
              <button
                key={tool.id}
                onClick={() => setChaosTool(tool.id)}
                title={tool.desc}
                className={`px-2.5 py-1 text-xs rounded transition-colors cursor-pointer border ${
                  chaosTool === tool.id
                    ? "text-white border-white/30"
                    : "bg-zinc-800 text-zinc-400 hover:bg-zinc-700 border-zinc-700"
                }`}
                style={chaosTool === tool.id ? { backgroundColor: tool.color } : undefined}
              >
                {tool.label}
              </button>
            ))}
          </div>

          {/* Actions row */}
          <div className="flex flex-wrap justify-center gap-2">
            {hasC4 && (
              <button
                onClick={handleDetonateC4}
                className="px-4 py-1.5 rounded-lg text-sm font-medium bg-red-600 hover:bg-red-500 text-white border border-red-500 transition-colors cursor-pointer animate-pulse"
              >
                Detonate C4
              </button>
            )}
            {hasLaunchers && (
              <button
                onClick={handleClearLaunchers}
                className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-emerald-400 hover:bg-zinc-700 border border-emerald-700 transition-colors cursor-pointer"
              >
                Clear Launchers
              </button>
            )}
            <button
              onClick={handleReset}
              className="px-3 py-1.5 rounded-lg text-sm bg-zinc-800 text-zinc-400 hover:bg-zinc-700 border border-zinc-700 transition-colors cursor-pointer"
            >
              Clear World
            </button>
          </div>
        </>
      )}

      {/* Canvas */}
      <div className="w-full max-w-[960px] relative overflow-hidden rounded-lg border border-zinc-700">
        <canvas
          ref={canvasRef}
          className="bg-zinc-900 w-full block"
          style={{
            touchAction: "none",
            imageRendering: "pixelated",
            cursor: mode === "chaos"
              ? (aiming ? "grabbing" : "crosshair")
              : "pointer",
          }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onContextMenu={onContextMenu}
        />

        {/* Entity count overlay */}
        {status === "running" && entityCount > 0 && (
          <div className="absolute top-2 right-2 bg-black/60 backdrop-blur-sm rounded-lg border border-zinc-600 px-3 py-1.5 pointer-events-none">
            <span className="text-xs text-zinc-300 tabular-nums font-mono">
              {entityCount} active
            </span>
          </div>
        )}
      </div>

      {status === "running" && (
        <p className="text-zinc-500 text-xs text-center px-2">
          {getHint()}
        </p>
      )}
    </div>
  );
}
