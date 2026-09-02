// src-tauri/src/audio/wav_writer.rs

use std::io::{self, BufWriter};
use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

/// Maps a hound error into an `io::Error` so `?` works throughout.
fn hound_to_io(error: hound::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, error.to_string())
}

/// Target audio format required by the WhisperX pipeline:
/// 16 kHz, mono, 16-bit signed PCM.
pub const SAMPLE_RATE: u32 = 16_000;
pub const CHANNELS: u16 = 1;
pub const BITS_PER_SAMPLE: u16 = 16;

/// Streaming WAV writer. Samples are appended as they arrive from the
/// cpal input callback; `finalize` fixes up the RIFF header on close.
pub struct WavWriterHandle {
    writer: Option<WavWriter<BufWriter<std::fs::File>>>,
    samples_written: u64,
}

impl WavWriterHandle {
    pub fn create(path: &Path) -> io::Result<Self> {
        let spec = WavSpec {
            channels: CHANNELS,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: BITS_PER_SAMPLE,
            sample_format: SampleFormat::Int,
        };

        let writer = WavWriter::create(path, spec).map_err(hound_to_io)?;

        Ok(Self {
            writer: Some(writer),
            samples_written: 0,
        })
    }

    /// Append PCM samples to the file. Returns `Err` after finalization.
    pub fn write_samples(&mut self, samples: &[i16]) -> io::Result<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "WAV writer already finalized")
        })?;

        for &sample in samples {
            writer.write_sample(sample).map_err(hound_to_io)?;
        }

        self.samples_written += samples.len() as u64;
        Ok(())
    }

    /// Flush and finalize the WAV header. Returns total samples written.
    pub fn finalize(mut self) -> io::Result<u64> {
        if let Some(writer) = self.writer.take() {
            writer.finalize().map_err(hound_to_io)?;
        }

        Ok(self.samples_written)
    }

    pub fn samples_written(&self) -> u64 {
        self.samples_written
    }
}