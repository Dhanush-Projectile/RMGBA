/* emulator.rs
 *
 * Copyright 2026 Dhanush
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use gtk::glib;
use mgba::Core;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapProd, HeapRb};

const VOLUME: f32 = 1.0;
const TARGET_FPS: f64 = 60.0;

pub const KEY_A: u32 = 1 << 0;
pub const KEY_B: u32 = 1 << 1;
pub const KEY_SELECT: u32 = 1 << 2;
pub const KEY_START: u32 = 1 << 3;
pub const KEY_RIGHT: u32 = 1 << 4;
pub const KEY_LEFT: u32 = 1 << 5;
pub const KEY_UP: u32 = 1 << 6;
pub const KEY_DOWN: u32 = 1 << 7;
pub const KEY_R: u32 = 1 << 8;
pub const KEY_L: u32 = 1 << 9;

/// Events sent from the emulation thread to the UI thread.
#[derive(Debug)]
pub enum EmuEvent {
    /// One full frame of pixels in 0x00RRGGBB layout (BGRA8 when read as bytes).
    Frame(Vec<u32>),
    Error(String),
}

/// Handler invoked on the UI (main) thread for every event.
type EventHandler = Arc<dyn Fn(EmuEvent) + Send + Sync>;

struct Resampler {
    step: f64,
    frac: f64,
    last: [f32; 2],
}

impl Resampler {
    fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            step: f64::from(input_rate) / f64::from(output_rate),
            frac: 0.0,
            last: [0.0, 0.0],
        }
    }

    fn set_input_rate(&mut self, input_rate: u32, output_rate: u32) {
        self.step = f64::from(input_rate) / f64::from(output_rate);
    }

    fn push(&mut self, input: &[i16], mut emit: impl FnMut(f32)) {
        for [left_in, right_in] in input.as_chunks::<2>().0 {
            let cur = [
                *left_in as f32 * VOLUME / 32768.0,
                *right_in as f32 * VOLUME / 32768.0,
            ];
            while self.frac < 1.0 {
                let t = self.frac as f32;
                let left = self.last[0] + (cur[0] - self.last[0]) * t;
                let right = self.last[1] + (cur[1] - self.last[1]) * t;
                emit(left.clamp(-1.0, 1.0));
                emit(right.clamp(-1.0, 1.0));
                self.frac += self.step;
            }
            self.frac -= 1.0;
            self.last = cur;
        }
    }
}

/// Owns the emulation thread and its audio output stream.
///
/// Dropping this stops emulation and closes the audio stream.
pub struct Emulator {
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Emulator {
    /// Spawns the emulation thread for `rom`.
    ///
    /// Audio output is set up on the emulation thread; if it is unavailable
    /// (e.g. no sound server in a sandbox), emulation continues muted and an
    /// error event reports it. `on_event` is invoked on the default main
    /// context (i.e. the UI thread) for every frame and error. `keys` is
    /// polled once per frame.
    pub fn start(
        rom: PathBuf,
        save: Option<PathBuf>,
        keys: Arc<AtomicU32>,
        on_event: EventHandler,
    ) -> Result<Self, String> {
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);

        let thread = std::thread::Builder::new()
            .name("emulation".into())
            .spawn(move || {
                // g_main_context_invoke is thread-safe and schedules the
                // closure on the main loop, marshalling events to the UI.
                let context = glib::MainContext::default();
                let send = |event: EmuEvent| {
                    let on_event = Arc::clone(&on_event);
                    context.invoke(move || on_event(event));
                };
                run_emulation(rom, save, keys, thread_running, &send);
            })
            .map_err(|e| format!("failed to spawn emulation thread: {e}"))?;

        Ok(Self {
            running,
            thread: Some(thread),
        })
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Output device for resampled audio; `producer` is `None` when muted.
struct AudioOutput {
    producer: Option<HeapProd<f32>>,
    sample_rate: u32,
    // Must be kept alive (or dropped on the thread) for playback to continue.
    _stream: Option<cpal::Stream>,
}

impl AudioOutput {
    fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no audio output device found")?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| format!("failed to query audio config: {e}"))?;
        if supported_config.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "unsupported audio sample format {:?}",
                supported_config.sample_format()
            ));
        }
        let mut config = supported_config.config();
        config.channels = 2; // Force stereo

        let sample_rate = config.sample_rate;
        let ring_buffer = HeapRb::<f32>::new((sample_rate / 10 * 2) as usize);
        let (producer, mut consumer) = ring_buffer.split();

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    for sample in data.iter_mut() {
                        *sample = consumer.try_pop().unwrap_or(0.0);
                    }
                },
                |err| eprintln!("Audio stream error: {err}"),
                None,
            )
            .map_err(|e| format!("failed to build audio stream: {e}"))?;
        stream.play().map_err(|e| format!("failed to play audio: {e}"))?;

        Ok(Self {
            producer: Some(producer),
            sample_rate,
            _stream: Some(stream),
        })
    }

    /// Silent fallback used when no sound server is reachable.
    fn dummy() -> Self {
        Self {
            producer: None,
            sample_rate: 48_000,
            _stream: None,
        }
    }

    fn emit(&mut self, sample: f32) {
        if let Some(producer) = self.producer.as_mut() {
            let _ = producer.try_push(sample);
        }
    }
}

fn run_emulation(
    rom: PathBuf,
    save: Option<PathBuf>,
    keys: Arc<AtomicU32>,
    running: Arc<AtomicBool>,
    send: &dyn Fn(EmuEvent),
) {
    let mut core = match Core::new() {
        Ok(core) => core,
        Err(err) => {
            send(EmuEvent::Error(format!("failed to create core: {err:?}")));
            return;
        }
    };
    if let Err(err) = core.load_rom_with_save(&rom, save.as_deref()) {
        send(EmuEvent::Error(format!("failed to load ROM: {err:?}")));
        return;
    }
    if let Err(err) = core.reset() {
        send(EmuEvent::Error(format!("failed to reset core: {err:?}")));
        return;
    }

    let mut audio = match AudioOutput::new() {
        Ok(audio) => audio,
        Err(err) => {
            // Keep emulating, just without sound.
            send(EmuEvent::Error(format!(
                "{err}; continuing without sound"
            )));
            AudioOutput::dummy()
        }
    };

    let mut resampler = Resampler::new(core.audio_sample_rate(), audio.sample_rate);
    let mut audio_scratch = vec![0i16; 4096];
    let frame_time = Duration::from_secs_f64(1.0 / TARGET_FPS);

    while running.load(Ordering::Relaxed) {
        let frame_start = Instant::now();

        if let Err(err) = core.set_keys(keys.load(Ordering::Relaxed)) {
            send(EmuEvent::Error(format!("set_keys failed: {err:?}")));
            return;
        }
        if let Err(err) = core.run_frame() {
            send(EmuEvent::Error(format!("run_frame failed: {err:?}")));
            return;
        }

        // Drain the emulated audio through the resampler into the output.
        resampler.set_input_rate(core.audio_sample_rate(), audio.sample_rate);
        loop {
            let count = core.read_audio(&mut audio_scratch);
            if count == 0 {
                break;
            }
            resampler.push(&audio_scratch[..count], |sample| audio.emit(sample));
        }

        send(EmuEvent::Frame(core.video_buffer().to_vec()));

        // libmgba runs unthrottled without an explicit sync source, so pace
        // the loop here to keep emulation near real-time speed.
        let spent = frame_start.elapsed();
        if spent < frame_time {
            std::thread::sleep(frame_time - spent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn core_produces_audio() {
        let mut core = Core::new().unwrap();
        core.load_rom(Path::new("game.gba")).unwrap();
        core.reset().unwrap();
        let mut buf = vec![0i16; 4096];
        for _ in 0..300 {
            core.run_frame().unwrap();
            let n = core.read_audio(&mut buf);
            if n > 0 && buf[..n].iter().any(|&s| s != 0) {
                return;
            }
        }
        panic!("no non-zero audio samples produced in 300 frames");
    }
}
