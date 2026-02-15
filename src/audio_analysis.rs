use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rodio::Source;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

/// Shared ring buffer for passing samples from the audio thread to the analyzer.
pub type SharedBuffer = Arc<Mutex<VecDeque<f32>>>;

pub fn new_shared_buffer() -> SharedBuffer {
    Arc::new(Mutex::new(VecDeque::with_capacity(8192)))
}

/// Audio features extracted from FFT analysis each tick.
#[derive(Copy, Clone, Debug, Default)]
pub struct AudioFeatures {
    pub rms: f32,
    pub bass: f32,
    #[allow(dead_code)]
    pub mid: f32,
    pub treble: f32,
    pub is_beat: bool,
}

// ---------------------------------------------------------------------------
// AnalyzingSource — wraps a Source<Item=f32>, copies samples to SharedBuffer
// ---------------------------------------------------------------------------

const FLUSH_INTERVAL: usize = 512;
const MAX_BUFFER_SAMPLES: usize = 16384;

pub struct AnalyzingSource<S: Source<Item = f32>> {
    inner: S,
    buffer: SharedBuffer,
    local_batch: Vec<f32>,
    channels: u16,
    #[allow(dead_code)]
    sample_rate: u32,
}

impl<S: Source<Item = f32>> AnalyzingSource<S> {
    pub fn new(inner: S, buffer: SharedBuffer, channels: u16, sample_rate: u32) -> Self {
        Self {
            inner,
            buffer,
            local_batch: Vec::with_capacity(FLUSH_INTERVAL * 2),
            channels,
            sample_rate,
        }
    }

    fn flush(&mut self) {
        if self.local_batch.is_empty() {
            return;
        }
        if let Ok(mut buf) = self.buffer.lock() {
            // Mix to mono if stereo
            if self.channels == 2 {
                for chunk in self.local_batch.chunks(2) {
                    let mono = if chunk.len() == 2 {
                        (chunk[0] + chunk[1]) * 0.5
                    } else {
                        chunk[0]
                    };
                    buf.push_back(mono);
                }
            } else {
                for &s in &self.local_batch {
                    buf.push_back(s);
                }
            }
            // Trim to max size
            while buf.len() > MAX_BUFFER_SAMPLES {
                buf.pop_front();
            }
        }
        self.local_batch.clear();
    }
}

impl<S: Source<Item = f32>> Iterator for AnalyzingSource<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        self.local_batch.push(sample);
        if self.local_batch.len() >= FLUSH_INTERVAL {
            self.flush();
        }
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source<Item = f32>> Source for AnalyzingSource<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.local_batch.clear();
        // Clear the shared buffer too so stale samples don't persist
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }
        self.inner.try_seek(pos)
    }
}

// ---------------------------------------------------------------------------
// AudioAnalyzer — reads SharedBuffer, runs FFT, extracts features
// ---------------------------------------------------------------------------

const FFT_SIZE: usize = 2048;

pub struct AudioAnalyzer {
    buffer: SharedBuffer,
    planner: FftPlanner<f32>,
    sample_rate: u32,
    // Beat detection state
    bass_history: VecDeque<f32>,
    last_beat: Instant,
}

impl AudioAnalyzer {
    pub fn new(buffer: SharedBuffer, sample_rate: u32) -> Self {
        Self {
            buffer,
            planner: FftPlanner::new(),
            sample_rate,
            bass_history: VecDeque::with_capacity(20),
            last_beat: Instant::now() - std::time::Duration::from_secs(1),
        }
    }

    pub fn analyze(&mut self) -> AudioFeatures {
        // Read samples from shared buffer
        let samples: Vec<f32> = {
            let buf = match self.buffer.lock() {
                Ok(b) => b,
                Err(_) => return AudioFeatures::default(),
            };
            if buf.len() < FFT_SIZE {
                return AudioFeatures::default();
            }
            // Take the most recent FFT_SIZE samples
            buf.iter().rev().take(FFT_SIZE).copied().collect::<Vec<_>>().into_iter().rev().collect()
        };

        // Compute RMS
        let rms_raw: f32 = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        let rms = (rms_raw * 4.0).min(1.0); // Scale up for visibility

        // Apply Hann window and prepare FFT input
        let fft = self.planner.plan_fft_forward(FFT_SIZE);
        let mut fft_input: Vec<Complex<f32>> = samples
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos());
                Complex::new(s * window, 0.0)
            })
            .collect();

        fft.process(&mut fft_input);

        // Compute magnitude spectrum (only first half — Nyquist)
        let bin_width = self.sample_rate as f32 / FFT_SIZE as f32;
        let nyquist_bins = FFT_SIZE / 2;

        let magnitudes: Vec<f32> = fft_input[..nyquist_bins]
            .iter()
            .map(|c| c.norm() / FFT_SIZE as f32)
            .collect();

        // Frequency band energy
        let bass_start = (20.0 / bin_width) as usize;
        let bass_end = (250.0 / bin_width) as usize;
        let mid_start = bass_end;
        let mid_end = (4000.0 / bin_width) as usize;
        let treble_start = mid_end;
        let treble_end = (16000.0 / bin_width).min(nyquist_bins as f32) as usize;

        let band_energy = |start: usize, end: usize| -> f32 {
            let start = start.min(magnitudes.len());
            let end = end.min(magnitudes.len());
            if start >= end {
                return 0.0;
            }
            magnitudes[start..end].iter().map(|m| m * m).sum::<f32>().sqrt()
        };

        let bass_raw = band_energy(bass_start, bass_end);
        let mid_raw = band_energy(mid_start, mid_end);
        let treble_raw = band_energy(treble_start, treble_end);

        // Normalize band energies (scale factors tuned for visibility)
        let bass = (bass_raw * 15.0).min(1.0);
        let mid = (mid_raw * 8.0).min(1.0);
        let treble = (treble_raw * 20.0).min(1.0);

        // Beat detection: bass spike vs rolling average
        self.bass_history.push_back(bass);
        if self.bass_history.len() > 20 {
            self.bass_history.pop_front();
        }

        let avg_bass = self.bass_history.iter().sum::<f32>() / self.bass_history.len() as f32;
        let beat_cooldown = std::time::Duration::from_millis(200);
        let is_beat = bass > avg_bass * 1.5
            && bass > 0.15
            && self.last_beat.elapsed() > beat_cooldown;

        if is_beat {
            self.last_beat = Instant::now();
        }

        AudioFeatures {
            rms,
            bass,
            mid,
            treble,
            is_beat,
        }
    }
}
