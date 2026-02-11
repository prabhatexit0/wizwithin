use rand::Rng;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// CHIP-8 constants
// ---------------------------------------------------------------------------
const RAM_SIZE: usize = 4096;
const NUM_REGS: usize = 16;
const STACK_SIZE: usize = 16;
const DISPLAY_WIDTH: usize = 64;
const DISPLAY_HEIGHT: usize = 32;
const DISPLAY_SIZE: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;
const NUM_KEYS: usize = 16;
const PROGRAM_START: u16 = 0x200;

// ---------------------------------------------------------------------------
// Built-in font set — each character is 5 bytes (4x5 pixels).
// Stored at 0x000..0x050 in RAM per the CHIP-8 spec.
// ---------------------------------------------------------------------------
const FONTSET: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

// ---------------------------------------------------------------------------
// Cpu — the complete CHIP-8 interpreter state.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct Cpu {
    ram: [u8; RAM_SIZE],
    v: [u8; NUM_REGS],
    i_reg: u16,
    pc: u16,
    stack: [u16; STACK_SIZE],
    sp: u16,
    delay_timer: u8,
    sound_timer: u8,
    display: [u8; DISPLAY_SIZE],
    keys: [bool; NUM_KEYS],
    waiting_for_key: bool,
    key_register: u8,
}

// ---------------------------------------------------------------------------
// Public API — exposed to JavaScript via wasm-bindgen.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
impl Cpu {
    /// Create a fresh CHIP-8 CPU with the fontset loaded and PC at 0x200.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Cpu {
        let mut cpu = Cpu {
            ram: [0; RAM_SIZE],
            v: [0; NUM_REGS],
            i_reg: 0,
            pc: PROGRAM_START,
            stack: [0; STACK_SIZE],
            sp: 0,
            delay_timer: 0,
            sound_timer: 0,
            display: [0; DISPLAY_SIZE],
            keys: [false; NUM_KEYS],
            waiting_for_key: false,
            key_register: 0,
        };
        cpu.ram[..FONTSET.len()].copy_from_slice(&FONTSET);
        cpu
    }

    /// Load a ROM into memory starting at 0x200.
    pub fn load_rom(&mut self, rom: &[u8]) {
        let start = PROGRAM_START as usize;
        let end = (start + rom.len()).min(RAM_SIZE);
        self.ram[start..end].copy_from_slice(&rom[..end - start]);
    }

    /// Hard-reset the CPU to power-on state (keeps fontset, clears everything
    /// else). Call `load_rom` again after this.
    pub fn reset(&mut self) {
        self.ram = [0; RAM_SIZE];
        self.ram[..FONTSET.len()].copy_from_slice(&FONTSET);
        self.v = [0; NUM_REGS];
        self.i_reg = 0;
        self.pc = PROGRAM_START;
        self.stack = [0; STACK_SIZE];
        self.sp = 0;
        self.delay_timer = 0;
        self.sound_timer = 0;
        self.display = [0; DISPLAY_SIZE];
        self.keys = [false; NUM_KEYS];
        self.waiting_for_key = false;
        self.key_register = 0;
    }

    // -----------------------------------------------------------------------
    // Timers — should be called at 60 Hz from JS.
    // -----------------------------------------------------------------------

    pub fn tick_timers(&mut self) {
        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
        if self.sound_timer > 0 {
            self.sound_timer -= 1;
        }
    }

    pub fn sound_active(&self) -> bool {
        self.sound_timer > 0
    }

    // -----------------------------------------------------------------------
    // CPU cycle — the Fetch-Decode-Execute heart of the emulator.
    // Call this ~500 times per second from JS for authentic speed.
    // -----------------------------------------------------------------------

    pub fn tick_cpu(&mut self) {
        if self.waiting_for_key {
            return;
        }

        let opcode = self.fetch();
        self.execute(opcode);
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    pub fn key_down(&mut self, key: u8) {
        if (key as usize) < NUM_KEYS {
            self.keys[key as usize] = true;

            if self.waiting_for_key {
                self.v[self.key_register as usize] = key;
                self.waiting_for_key = false;
            }
        }
    }

    pub fn key_up(&mut self, key: u8) {
        if (key as usize) < NUM_KEYS {
            self.keys[key as usize] = false;
        }
    }

    // -----------------------------------------------------------------------
    // Display buffer — zero-copy access from JS.
    //
    // Each pixel is a u8 (0x00 off, 0xFF on) so JS can build an RGBA
    // ImageData cheaply: for each byte, write [val, val, val, 0xFF].
    // -----------------------------------------------------------------------

    pub fn display_ptr(&self) -> *const u8 {
        self.display.as_ptr()
    }

    pub fn display_len(&self) -> usize {
        DISPLAY_SIZE
    }

    pub fn display_width(&self) -> usize {
        DISPLAY_WIDTH
    }

    pub fn display_height(&self) -> usize {
        DISPLAY_HEIGHT
    }
}

// ---------------------------------------------------------------------------
// Private — Fetch, Decode, Execute
// ---------------------------------------------------------------------------
impl Cpu {
    /// Fetch the 2-byte opcode at PC and advance PC by 2.
    #[inline]
    fn fetch(&mut self) -> u16 {
        let hi = self.ram[self.pc as usize] as u16;
        let lo = self.ram[(self.pc as usize) + 1] as u16;
        self.pc += 2;
        (hi << 8) | lo
    }

    /// Decode and execute a single opcode.
    ///
    /// We destructure each 16-bit instruction into nibbles for clean matching:
    ///
    ///   opcode = 0xABCD
    ///   nib1 = A  (top 4 bits — the "category")
    ///   nib2 = B  (often Vx register index)
    ///   nib3 = C  (often Vy register index)
    ///   nib4 = D  (lowest nibble, sometimes N)
    ///   nnn  = BCD (12-bit address)
    ///   kk   = CD  (8-bit immediate)
    ///
    fn execute(&mut self, op: u16) {
        let nib1 = ((op & 0xF000) >> 12) as u8;
        let nib2 = ((op & 0x0F00) >> 8) as u8;
        let nib3 = ((op & 0x00F0) >> 4) as u8;
        let nib4 = (op & 0x000F) as u8;

        let x = nib2 as usize;   // Vx register index
        let y = nib3 as usize;   // Vy register index
        let nnn = op & 0x0FFF;   // 12-bit address
        let kk = (op & 0x00FF) as u8; // 8-bit immediate
        let n = nib4;             // lowest nibble

        match (nib1, nib2, nib3, nib4) {
            // =================================================================
            // 0x0___  —  System / Display
            // =================================================================

            // 00E0 — CLS: Clear display
            (0x0, 0x0, 0xE, 0x0) => {
                self.display.fill(0);
            }

            // 00EE — RET: Return from subroutine
            (0x0, 0x0, 0xE, 0xE) => {
                self.sp -= 1;
                self.pc = self.stack[self.sp as usize];
            }

            // 0NNN — SYS addr (ignored on modern interpreters)
            (0x0, _, _, _) => {}

            // =================================================================
            // 0x1___  —  Flow control
            // =================================================================

            // 1NNN — JP addr: Jump to address NNN
            (0x1, _, _, _) => {
                self.pc = nnn;
            }

            // =================================================================
            // 0x2___  —  Subroutine
            // =================================================================

            // 2NNN — CALL addr: Call subroutine at NNN
            (0x2, _, _, _) => {
                self.stack[self.sp as usize] = self.pc;
                self.sp += 1;
                self.pc = nnn;
            }

            // =================================================================
            // 0x3___  —  Conditional skip (register == immediate)
            // =================================================================

            // 3XKK — SE Vx, byte: Skip next if Vx == KK
            (0x3, _, _, _) => {
                if self.v[x] == kk {
                    self.pc += 2;
                }
            }

            // =================================================================
            // 0x4___  —  Conditional skip (register != immediate)
            // =================================================================

            // 4XKK — SNE Vx, byte: Skip next if Vx != KK
            (0x4, _, _, _) => {
                if self.v[x] != kk {
                    self.pc += 2;
                }
            }

            // =================================================================
            // 0x5___  —  Conditional skip (register == register)
            // =================================================================

            // 5XY0 — SE Vx, Vy: Skip next if Vx == Vy
            (0x5, _, _, 0x0) => {
                if self.v[x] == self.v[y] {
                    self.pc += 2;
                }
            }

            // =================================================================
            // 0x6___  —  Load immediate
            // =================================================================

            // 6XKK — LD Vx, byte: Set Vx = KK
            (0x6, _, _, _) => {
                self.v[x] = kk;
            }

            // =================================================================
            // 0x7___  —  Add immediate (no carry flag)
            // =================================================================

            // 7XKK — ADD Vx, byte: Set Vx = Vx + KK (wrapping)
            (0x7, _, _, _) => {
                self.v[x] = self.v[x].wrapping_add(kk);
            }

            // =================================================================
            // 0x8___  —  ALU operations (register-register)
            // =================================================================

            // 8XY0 — LD Vx, Vy
            (0x8, _, _, 0x0) => {
                self.v[x] = self.v[y];
            }

            // 8XY1 — OR Vx, Vy
            (0x8, _, _, 0x1) => {
                self.v[x] |= self.v[y];
                self.v[0xF] = 0; // quirk: VF reset
            }

            // 8XY2 — AND Vx, Vy
            (0x8, _, _, 0x2) => {
                self.v[x] &= self.v[y];
                self.v[0xF] = 0;
            }

            // 8XY3 — XOR Vx, Vy
            (0x8, _, _, 0x3) => {
                self.v[x] ^= self.v[y];
                self.v[0xF] = 0;
            }

            // 8XY4 — ADD Vx, Vy (VF = carry)
            (0x8, _, _, 0x4) => {
                let (result, overflow) = self.v[x].overflowing_add(self.v[y]);
                self.v[x] = result;
                self.v[0xF] = if overflow { 1 } else { 0 };
            }

            // 8XY5 — SUB Vx, Vy (VF = NOT borrow)
            (0x8, _, _, 0x5) => {
                let (result, borrow) = self.v[x].overflowing_sub(self.v[y]);
                self.v[x] = result;
                self.v[0xF] = if borrow { 0 } else { 1 };
            }

            // 8XY6 — SHR Vx {, Vy} (VF = shifted-out bit)
            (0x8, _, _, 0x6) => {
                let lsb = self.v[y] & 1;
                self.v[x] = self.v[y] >> 1;
                self.v[0xF] = lsb;
            }

            // 8XY7 — SUBN Vx, Vy (VF = NOT borrow)
            (0x8, _, _, 0x7) => {
                let (result, borrow) = self.v[y].overflowing_sub(self.v[x]);
                self.v[x] = result;
                self.v[0xF] = if borrow { 0 } else { 1 };
            }

            // 8XYE — SHL Vx {, Vy} (VF = shifted-out bit)
            (0x8, _, _, 0xE) => {
                let msb = (self.v[y] >> 7) & 1;
                self.v[x] = self.v[y] << 1;
                self.v[0xF] = msb;
            }

            // =================================================================
            // 0x9___  —  Conditional skip (register != register)
            // =================================================================

            // 9XY0 — SNE Vx, Vy: Skip next if Vx != Vy
            (0x9, _, _, 0x0) => {
                if self.v[x] != self.v[y] {
                    self.pc += 2;
                }
            }

            // =================================================================
            // 0xA___  —  Set index register
            // =================================================================

            // ANNN — LD I, addr: Set I = NNN
            (0xA, _, _, _) => {
                self.i_reg = nnn;
            }

            // =================================================================
            // 0xB___  —  Jump with offset
            // =================================================================

            // BNNN — JP V0, addr: Jump to NNN + V0
            (0xB, _, _, _) => {
                self.pc = nnn + self.v[0] as u16;
            }

            // =================================================================
            // 0xC___  —  Random
            // =================================================================

            // CXKK — RND Vx, byte: Vx = random() AND KK
            (0xC, _, _, _) => {
                let r: u8 = rand::rng().random();
                self.v[x] = r & kk;
            }

            // =================================================================
            // 0xD___  —  Draw sprite (the big one)
            // =================================================================

            // DXYN — DRW Vx, Vy, N: Draw N-byte sprite at (Vx, Vy).
            // XOR onto the display. VF = 1 if any pixel was erased.
            (0xD, _, _, _) => {
                let x_coord = self.v[x] as usize % DISPLAY_WIDTH;
                let y_coord = self.v[y] as usize % DISPLAY_HEIGHT;
                self.v[0xF] = 0;

                for row in 0..n as usize {
                    let py = y_coord + row;
                    if py >= DISPLAY_HEIGHT {
                        break;
                    }

                    let sprite_byte = self.ram[self.i_reg as usize + row];

                    for col in 0..8usize {
                        let px = x_coord + col;
                        if px >= DISPLAY_WIDTH {
                            break;
                        }

                        let sprite_pixel = (sprite_byte >> (7 - col)) & 1;
                        if sprite_pixel == 1 {
                            let idx = py * DISPLAY_WIDTH + px;
                            if self.display[idx] != 0 {
                                self.v[0xF] = 1; // collision
                            }
                            self.display[idx] ^= 0xFF;
                        }
                    }
                }
            }

            // =================================================================
            // 0xE___  —  Key-press conditionals
            // =================================================================

            // EX9E — SKP Vx: Skip next if key Vx is pressed
            (0xE, _, 0x9, 0xE) => {
                if self.keys[self.v[x] as usize & 0xF] {
                    self.pc += 2;
                }
            }

            // EXA1 — SKNP Vx: Skip next if key Vx is NOT pressed
            (0xE, _, 0xA, 0x1) => {
                if !self.keys[self.v[x] as usize & 0xF] {
                    self.pc += 2;
                }
            }

            // =================================================================
            // 0xF___  —  Misc / Timers / Memory / BCD
            // =================================================================

            // FX07 — LD Vx, DT: Set Vx = delay timer
            (0xF, _, 0x0, 0x7) => {
                self.v[x] = self.delay_timer;
            }

            // FX0A — LD Vx, K: Wait for key press, store in Vx
            (0xF, _, 0x0, 0xA) => {
                self.waiting_for_key = true;
                self.key_register = x as u8;
            }

            // FX15 — LD DT, Vx: Set delay timer = Vx
            (0xF, _, 0x1, 0x5) => {
                self.delay_timer = self.v[x];
            }

            // FX18 — LD ST, Vx: Set sound timer = Vx
            (0xF, _, 0x1, 0x8) => {
                self.sound_timer = self.v[x];
            }

            // FX1E — ADD I, Vx: Set I = I + Vx
            (0xF, _, 0x1, 0xE) => {
                self.i_reg = self.i_reg.wrapping_add(self.v[x] as u16);
            }

            // FX29 — LD F, Vx: Set I = location of font sprite for digit Vx
            (0xF, _, 0x2, 0x9) => {
                self.i_reg = (self.v[x] as u16 & 0xF) * 5;
            }

            // FX33 — LD B, Vx: Store BCD of Vx at I, I+1, I+2
            (0xF, _, 0x3, 0x3) => {
                let val = self.v[x];
                let base = self.i_reg as usize;
                self.ram[base] = val / 100;
                self.ram[base + 1] = (val / 10) % 10;
                self.ram[base + 2] = val % 10;
            }

            // FX55 — LD [I], Vx: Store V0..Vx in memory starting at I
            (0xF, _, 0x5, 0x5) => {
                let base = self.i_reg as usize;
                for reg in 0..=x {
                    self.ram[base + reg] = self.v[reg];
                }
                self.i_reg += x as u16 + 1; // original COSMAC VIP behavior
            }

            // FX65 — LD Vx, [I]: Read V0..Vx from memory starting at I
            (0xF, _, 0x6, 0x5) => {
                let base = self.i_reg as usize;
                for reg in 0..=x {
                    self.v[reg] = self.ram[base + reg];
                }
                self.i_reg += x as u16 + 1;
            }

            // Unknown opcode — silently ignore (no panic in WASM).
            _ => {}
        }
    }
}
