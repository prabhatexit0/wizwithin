use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// Waveform types – stored as u8 for compact WASM-to-JS interop.
// ---------------------------------------------------------------------------
const SINE: u8 = 0;
const SQUARE: u8 = 1;
const SAWTOOTH: u8 = 2;
const TRIANGLE: u8 = 3;

// ---------------------------------------------------------------------------
// Oscillator – a single voice that generates audio samples.
//
// ## The Phase Accumulator (key DSP concept)
//
// Digital audio works by computing discrete samples at a fixed rate (e.g.
// 44 100 samples/second).  To generate a periodic waveform we need to know
// "where we are" inside one cycle at every sample.  That's what `phase` does.
//
// `phase` is a number in the range [0.0, 1.0) representing our position
// within one full cycle of the waveform:
//   - 0.0 = beginning of the cycle
//   - 0.5 = halfway through the cycle
//   - 1.0 = end of the cycle (wraps back to 0.0)
//
// Each sample we advance `phase` by `frequency / sample_rate`.  This is the
// **phase increment** (Δφ).  For example, a 440 Hz sine wave at 44 100 Hz
// sample rate advances by 440/44100 ≈ 0.00998 per sample, completing one
// full cycle every ~100 samples.
//
// Because `phase` stays in [0.0, 1.0) regardless of frequency, switching
// between waveform shapes mid-stream is glitch-free — we just evaluate a
// different function at the current phase.
// ---------------------------------------------------------------------------

/// Oscillator generates a periodic waveform sample-by-sample.
struct Oscillator {
    /// Current position within one cycle, range [0.0, 1.0).
    phase: f32,
    /// How many cycles per second (pitch).  Human hearing: ~20 Hz – 20 kHz.
    frequency: f32,
    /// Samples per second, typically 44 100 or 48 000.
    sample_rate: f32,
    /// Which waveform shape to generate (Sine, Square, Sawtooth, Triangle).
    waveform: u8,
}

impl Oscillator {
    fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            frequency: 440.0,
            sample_rate,
            waveform: SINE,
        }
    }

    /// Compute the next audio sample and advance the phase accumulator.
    ///
    /// Returns a value in [-1.0, 1.0] — the standard range for audio samples.
    #[inline]
    fn next_sample(&mut self) -> f32 {
        // Evaluate the waveform at the current phase position.
        let sample = match self.waveform {
            // ---------------------------------------------------------------
            // Sine wave – the purest tone, a single frequency with no
            // harmonics.  sin(2π · phase) maps [0,1) → one full cycle.
            // ---------------------------------------------------------------
            SINE => {
                let theta = self.phase * std::f32::consts::TAU; // TAU = 2π
                theta.sin()
            }

            // ---------------------------------------------------------------
            // Square wave – alternates between +1 and -1 at the halfway
            // point.  Rich in odd harmonics (3rd, 5th, 7th …), giving it a
            // hollow, buzzy character.
            // ---------------------------------------------------------------
            SQUARE => {
                if self.phase < 0.5 { 1.0 } else { -1.0 }
            }

            // ---------------------------------------------------------------
            // Sawtooth wave – ramps linearly from -1 to +1 then snaps back.
            // Contains all harmonics (even and odd), creating a bright,
            // buzzy timbre.  Great for basses and leads.
            // ---------------------------------------------------------------
            SAWTOOTH => {
                2.0 * self.phase - 1.0
            }

            // ---------------------------------------------------------------
            // Triangle wave – like a sawtooth folded back on itself.  Ramps
            // up from -1 to +1 in the first half, then back down to -1.
            // Contains only odd harmonics that roll off faster than square,
            // giving a softer, flute-like sound.
            // ---------------------------------------------------------------
            TRIANGLE => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0       // -1 → +1 over first half
                } else {
                    -4.0 * self.phase + 3.0      // +1 → -1 over second half
                }
            }

            _ => 0.0,
        };

        // Advance the phase accumulator.
        //
        // phase_increment = frequency / sample_rate
        //   e.g. 440 Hz / 44100 = 0.00998 — we move ~1% through the cycle
        //   per sample, completing a full cycle 440 times per second.
        //
        // The modulo (fract) keeps phase in [0.0, 1.0) so it wraps smoothly
        // without overflow, no matter how long the synth runs.
        self.phase = (self.phase + self.frequency / self.sample_rate).fract();

        sample
    }
}

// ---------------------------------------------------------------------------
// Synth – the engine exposed to JavaScript.
//
// Holds an oscillator plus a gain (volume) control and an internal buffer
// that JS reads from via a shared-memory pointer.
// ---------------------------------------------------------------------------
#[wasm_bindgen]
pub struct Synth {
    osc: Oscillator,
    /// Volume multiplier in [0.0, 1.0].
    gain: f32,
    /// Internal audio buffer.  JS calls `fill_buffer()` to populate it,
    /// then reads the samples via `buffer_ptr()` / `buffer_len()`.
    buffer: Vec<f32>,
}

#[wasm_bindgen]
impl Synth {
    // -- Constructor --------------------------------------------------------

    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32, buffer_size: usize) -> Synth {
        Synth {
            osc: Oscillator::new(sample_rate),
            gain: 0.0,
            buffer: vec![0.0; buffer_size],
        }
    }

    // -- Parameter setters --------------------------------------------------

    /// Set the oscillator frequency (pitch) in Hz.
    /// The XY pad maps the X axis to roughly 100–800 Hz.
    pub fn set_frequency(&mut self, freq: f32) {
        self.osc.frequency = freq;
    }

    /// Set the output gain (volume) in [0.0, 1.0].
    /// The XY pad maps the Y axis to this range.
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain.clamp(0.0, 1.0);
    }

    /// Set the waveform type: 0 = Sine, 1 = Square, 2 = Sawtooth, 3 = Triangle.
    pub fn set_waveform(&mut self, waveform: u8) {
        if waveform <= TRIANGLE {
            self.osc.waveform = waveform;
        }
    }

    // -- Getters (for UI feedback) ------------------------------------------

    pub fn frequency(&self) -> f32 {
        self.osc.frequency
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn waveform(&self) -> u8 {
        self.osc.waveform
    }

    // -- Audio buffer -------------------------------------------------------

    /// Fill the internal buffer with the next chunk of audio samples.
    ///
    /// This is the hot loop — called ~44100/bufferSize times per second.
    /// Each call generates `buffer_size` samples by:
    ///   1. Asking the oscillator for the next raw sample.
    ///   2. Multiplying by `gain` to control volume.
    ///   3. Writing the result into the contiguous f32 buffer.
    ///
    /// The JS side reads this buffer via `buffer_ptr()` — a zero-copy view
    /// into WASM linear memory, avoiding expensive data marshalling.
    pub fn fill_buffer(&mut self) {
        let len = self.buffer.len();
        let gain = self.gain;
        for i in 0..len {
            let sample = self.osc.next_sample();
            self.buffer[i] = sample * gain;
        }
    }

    /// Pointer to the start of the f32 audio buffer in WASM linear memory.
    /// JS constructs `new Float32Array(memory.buffer, ptr, len)` from this.
    pub fn buffer_ptr(&self) -> *const f32 {
        self.buffer.as_ptr()
    }

    /// Number of f32 samples in the buffer.
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}
