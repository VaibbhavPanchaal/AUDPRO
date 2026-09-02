// src-tauri/src/audio/recorder.rs

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};

use super::wav_writer::{WavWriterHandle, SAMPLE_RATE};

/// Soft ceiling on the inter-thread sample queue (~4 hours of audio).
/// The writer thread drains continuously, so this is a safety valve only.
const MAX_QUEUED_SAMPLES: usize = SAMPLE_RATE as usize * 60 * 60 * 4;
const WRITER_POLL_INTERVAL: Duration = Duration::from_millis(200);

type SharedQueue = Arc<(Mutex<VecDeque<i16>>, Condvar)>;

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Streaming linear-interpolation resampler that converts an arbitrary
/// input sample rate to the 16 kHz pipeline rate, one sample at a time.
struct Resampler {
    /// Input interval length per output sample (`in_rate / out_rate`).
    step: f64,
    /// Fractional position within the current `[prev, curr]` interval.
    pos: f64,
    prev: Option<f32>,
}

impl Resampler {
    fn new(input_rate: u32) -> Self {
        Self {
            step: input_rate as f64 / SAMPLE_RATE as f64,
            pos: 0.0,
            prev: None,
        }
    }

    fn process(&mut self, sample: f32, out: &mut Vec<i16>) {
        let prev = match self.prev {
            Some(prev) => prev,
            None => {
                self.prev = Some(sample);
                return;
            }
        };

        while self.pos < 1.0 {
            let t = self.pos as f32;
            let interpolated = prev + (sample - prev) * t;
            out.push(quantize(interpolated));
            self.pos += self.step;
        }

        self.pos -= 1.0;
        self.prev = Some(sample);
    }
}

fn quantize(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// Downmix any channel layout to mono f32.
fn mono_f32(data: &[f32], channels: u16) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1) as usize;

    data.chunks_exact(channels)
        .map(move |frame| frame.iter().sum::<f32>() / channels as f32)
}

fn mono_i16(data: &[i16], channels: u16) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1) as usize;

    data.chunks_exact(channels).map(move |frame| {
        frame
            .iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .sum::<f32>()
            / channels as f32
    })
}

fn mono_u16(data: &[u16], channels: u16) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1) as usize;

    data.chunks_exact(channels).map(move |frame| {
        frame
            .iter()
            .map(|&s| (s as f32 - 32_768.0) / 32_768.0)
            .sum::<f32>()
            / channels as f32
    })
}

/// Push converted samples to the writer queue, dropping the oldest data
/// if the queue exceeds its safety ceiling.
fn enqueue(queue: &SharedQueue, samples: Vec<i16>) {
    if samples.is_empty() {
        return;
    }

    let (mutex, condvar) = &**queue;
    let mut q = lock_recover(mutex);

    let projected = q.len() + samples.len();

    if projected > MAX_QUEUED_SAMPLES {
        let excess = (projected - MAX_QUEUED_SAMPLES).min(q.len());
        q.drain(..excess);
        eprintln!("Audio queue overflow; dropped {excess} oldest samples");
    }

    q.extend(samples);
    drop(q);
    condvar.notify_one();
}

/// Result reported back from the capture thread after stream setup.
enum CaptureInit {
    Ready,
    Failed(String),
}

/// Message emitted by the capture thread when it exits.
enum CaptureExit {
    Stopped,
    StreamError(String),
}

pub struct Recorder {
    output_path: PathBuf,
    queue: SharedQueue,
    stop_flag: Arc<AtomicBool>,
    stream_failed: Arc<AtomicBool>,
    capture_thread: Option<JoinHandle<CaptureExit>>,
    writer_thread: Option<JoinHandle<Result<u64, io::Error>>>,
    started_at: Instant,
    samples_written: u64,
}

impl Recorder {
    /// Start capturing the given device (by substring match, or the system
    /// default when `device_name` is `None`) into `<session_dir>/audio.wav`.
    /// Device-native rate/channel formats are converted to 16 kHz mono
    /// 16-bit PCM in software before hitting the disk.
    ///
    /// The cpal `Stream` is not `Send`, so it is created and owned by a
    /// dedicated capture thread; this struct only holds control flags.
    pub fn start(session_dir: &Path, device_name: Option<&str>) -> Result<Self, String> {
        std::fs::create_dir_all(session_dir)
            .map_err(|e| format!("Unable to create session directory: {e}"))?;

        let output_path = session_dir.join("audio.wav");
        let host = cpal::default_host();
        let (device, supported) = select_device(&host, device_name)?;

        let device_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let sample_format = supported.sample_format();
        let needs_resampling = device_rate != SAMPLE_RATE;
        let stream_config: StreamConfig = supported.config();

        let queue: SharedQueue = Arc::new((
            Mutex::new(VecDeque::with_capacity(16_384)),
            Condvar::new(),
        ));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stream_failed = Arc::new(AtomicBool::new(false));

        // Writer thread: continuously drains the queue to disk so file I/O
        // never blocks the realtime audio callback.
        let writer_queue = Arc::clone(&queue);
        let writer_stop = Arc::clone(&stop_flag);
        let writer_path = output_path.clone();

        let writer_thread = thread::Builder::new()
            .name("wav-writer".to_string())
            .spawn(move || -> Result<u64, io::Error> {
                let mut writer = WavWriterHandle::create(&writer_path)?;
                let mut pending: Vec<i16> = Vec::with_capacity(8192);

                loop {
                    {
                        let (mutex, condvar) = &*writer_queue;
                        let mut q = lock_recover(mutex);

                        loop {
                            if !q.is_empty() || writer_stop.load(Ordering::Acquire) {
                                break;
                            }

                            q = match condvar.wait_timeout(q, WRITER_POLL_INTERVAL) {
                                Ok((guard, _)) => guard,
                                Err(poisoned) => poisoned.into_inner().0,
                            };
                        }

                        pending.extend(q.drain(..));
                    }

                    if !pending.is_empty() {
                        writer.write_samples(&pending)?;
                        pending.clear();
                    }

                    if writer_stop.load(Ordering::Acquire) {
                        return writer.finalize();
                    }
                }
            })
            .map_err(|e| format!("Unable to spawn WAV writer thread: {e}"))?;

        // Capture thread: owns the non-Send cpal Stream for the lifetime of
        // the recording and converts/resamples incoming buffers.
        let (init_tx, init_rx) = mpsc::channel::<CaptureInit>();

        let capture_stop = Arc::clone(&stop_flag);
        let capture_failed = Arc::clone(&stream_failed);
        let capture_queue = Arc::clone(&queue);

        let capture_thread_result = thread::Builder::new()
            .name("audio-capture".to_string())
            .spawn(move || -> CaptureExit {
                let build_result = build_capture_stream(
                    &device,
                    sample_format,
                    &stream_config,
                    device_rate,
                    channels,
                    needs_resampling,
                    &capture_queue,
                    &capture_failed,
                );

                let stream: Stream = match build_result {
                    Ok(stream) => {
                        let _ = init_tx.send(CaptureInit::Ready);
                        stream
                    }
                    Err(e) => {
                        let _ = init_tx.send(CaptureInit::Failed(e));
                        return CaptureExit::StreamError(
                            "capture stream failed to start".into(),
                        );
                    }
                };

                if let Err(e) = stream.play() {
                    let _ = init_tx.send(CaptureInit::Failed(format!(
                        "Unable to start input stream: {e}"
                    )));
                    return CaptureExit::StreamError(e.to_string());
                }

                // Keep the stream alive until a stop is requested. The
                // realtime callbacks push samples into `capture_queue`.
                while !capture_stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(50));
                }

                CaptureExit::Stopped
            });

        let capture_thread = match capture_thread_result {
            Ok(handle) => handle,
            Err(e) => {
                stop_flag.store(true, Ordering::Release);
                let _ = writer_thread.join();
                return Err(format!("Unable to spawn capture thread: {e}"));
            }
        };

        // Wait for stream setup before reporting success to the caller.
        match init_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(CaptureInit::Ready) => {}
            Ok(CaptureInit::Failed(detail)) => {
                stop_flag.store(true, Ordering::Release);
                let _ = capture_thread.join();
                let _ = writer_thread.join();
                return Err(detail);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                stop_flag.store(true, Ordering::Release);
                let _ = capture_thread.join();
                let _ = writer_thread.join();
                return Err("Capture stream initialization timed out".into());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop_flag.store(true, Ordering::Release);
                let _ = capture_thread.join();
                let _ = writer_thread.join();
                return Err("Capture thread exited during initialization".into());
            }
        }

        Ok(Self {
            output_path,
            queue,
            stop_flag,
            stream_failed,
            capture_thread: Some(capture_thread),
            writer_thread: Some(writer_thread),
            started_at: Instant::now(),
            samples_written: 0,
        })
    }

    pub fn output_path(&self) -> &Path {
        &self.output_path
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Stop capture, flush the WAV file, and return its final path.
    pub fn stop(mut self) -> Result<String, String> {
        self.shutdown_internal()?;

        if self.stream_failed.load(Ordering::Acquire) {
            return Err("Audio input stream reported an error during recording".into());
        }

        if self.samples_written == 0 {
            return Err("No audio was captured".into());
        }

        Ok(self.output_path.to_string_lossy().into_owned())
    }

    fn shutdown_internal(&mut self) -> Result<(), String> {
        self.stop_flag.store(true, Ordering::Release);

        let (_, condvar) = &*self.queue;
        condvar.notify_all();

        // Join the capture thread first: it owns the stream, and dropping
        // the stream halts realtime callbacks before finalization.
        if let Some(handle) = self.capture_thread.take() {
            match handle.join() {
                Ok(CaptureExit::Stopped) => {}
                Ok(CaptureExit::StreamError(e)) => {
                    eprintln!("Capture thread ended with stream error: {e}");
                }
                Err(_) => return Err("Capture thread panicked".into()),
            }
        }

        if let Some(handle) = self.writer_thread.take() {
            match handle.join() {
                Ok(Ok(samples)) => self.samples_written = samples,
                Ok(Err(e)) => return Err(format!("WAV writer failed: {e}")),
                Err(_) => return Err("WAV writer thread panicked".into()),
            }
        }

        Ok(())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.shutdown_internal();
    }
}

#[allow(clippy::too_many_arguments)]
fn build_capture_stream(
    device: &Device,
    sample_format: SampleFormat,
    stream_config: &StreamConfig,
    device_rate: u32,
    channels: u16,
    needs_resampling: bool,
    queue: &SharedQueue,
    stream_failed: &Arc<AtomicBool>,
) -> Result<Stream, String> {
    let error_callback = {
        let failed = Arc::clone(stream_failed);
        move |err| {
            eprintln!("Audio input stream error: {err}");
            failed.store(true, Ordering::Release);
        }
    };

    macro_rules! stream_for {
        ($sample:ty, $mono_fn:ident) => {{
            let queue = Arc::clone(queue);
            let mut resampler = Resampler::new(device_rate);

            device.build_input_stream(
                stream_config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    let mut converted: Vec<i16> = Vec::with_capacity(data.len());

                    if needs_resampling {
                        for mono in $mono_fn(data, channels) {
                            resampler.process(mono, &mut converted);
                        }
                    } else {
                        for mono in $mono_fn(data, channels) {
                            converted.push(quantize(mono));
                        }
                    }

                    enqueue(&queue, converted);
                },
                error_callback,
                None,
            )
        }};
    }

    match sample_format {
        SampleFormat::F32 => stream_for!(f32, mono_f32).map_err(|e| e.to_string()),
        SampleFormat::I16 => stream_for!(i16, mono_i16).map_err(|e| e.to_string()),
        SampleFormat::U16 => stream_for!(u16, mono_u16).map_err(|e| e.to_string()),
        other => Err(format!("Unsupported input sample format: {other:?}")),
    }
}

/// Select the requested input device (by case-insensitive substring match
/// on its name) or fall back to the system default. The device is returned
/// with its native configuration; rate/channel conversion happens in
/// software inside the capture callback.
fn select_device(
    host: &cpal::Host,
    device_name: Option<&str>,
) -> Result<(Device, SupportedStreamConfig), String> {
    let candidate: Option<Device> = match device_name {
        Some(wanted) if !wanted.is_empty() => host
            .input_devices()
            .ok()
            .and_then(|devices| {
                let wanted = wanted.to_lowercase();

                devices
                    .filter_map(|d| d.name().ok().map(|name| (d, name)))
                    .find(|(_, name)| name.to_lowercase().contains(&wanted))
                    .map(|(d, _)| d)
            })
            .or_else(|| host.default_input_device()),
        _ => host.default_input_device(),
    };

    let device = candidate.ok_or_else(|| "No input device available".to_string())?;

    let config = device.default_input_config().map_err(|e| {
        format!(
            "Unable to read default input configuration for '{}': {e}",
            device.name().unwrap_or_default()
        )
    })?;

    Ok((device, config))
}

/// List available input device names (used by the `list_input_devices` command).
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();

    match host.input_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(error) => vec![format!("<device enumeration failed: {error}>")],
    }
}