//! Native microphone capture: the default input device, converted to 16 kHz mono PCM16 and handed
//! out in 1,024-byte frames (32 ms), the same frames the web view's capture sends.
//!
//! Why it exists: the web view only captures while its window is alive and in front, and its
//! microphone permission is the web view's. Capturing here keeps Pluto listening with the window
//! hidden, and uses the operating system's own microphone permission.
//!
//! Unverified on a real microphone: CI compiles it, nobody has spoken into it yet. The shell treats
//! it as a beta and falls back to the web view's capture when it cannot start.

use thiserror::Error;

/// Bytes in one frame: 512 samples, 32 ms at 16 kHz.
pub const FRAME_BYTES: usize = 1024;

#[derive(Debug, Error)]
pub enum MicError {
    #[error("Capturing the microphone natively is only available on Windows and Mac.")]
    Unsupported,
    #[error("Pluto could not open the microphone ({0}).")]
    Device(String),
}

/// Turns samples into whole frames, keeping the remainder for the next call.
#[derive(Default)]
pub struct Framer {
    pending: Vec<i16>,
}

impl Framer {
    pub fn push(&mut self, samples: &[i16]) -> Vec<Vec<u8>> {
        self.pending.extend_from_slice(samples);
        let per = FRAME_BYTES / 2;
        let mut out = Vec::new();
        while self.pending.len() >= per {
            let frame: Vec<u8> = self
                .pending
                .drain(..per)
                .flat_map(i16::to_le_bytes)
                .collect();
            out.push(frame);
        }
        out
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::MicError;

    pub struct Mic;

    impl Drop for Mic {
        fn drop(&mut self) {}
    }

    pub fn start(_on_frame: impl FnMut(Vec<u8>) + Send + 'static) -> Result<Mic, MicError> {
        Err(MicError::Unsupported)
    }
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;
    use pluto_loopback::convert::To16kMono;

    use super::{Framer, MicError};

    /// A running capture. Dropping it stops it.
    pub struct Mic {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for Mic {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn device_error(what: &str, e: impl std::fmt::Display) -> MicError {
        MicError::Device(format!("{what}: {e}"))
    }

    /// Starts capturing. `on_frame` gets each 1,024-byte frame on the audio thread, so it must
    /// not block. The stream lives on its own thread (it is not `Send` on every platform) and its
    /// set-up result comes back before this returns, so a failure is an `Err` here.
    pub fn start(on_frame: impl FnMut(Vec<u8>) + Send + 'static) -> Result<Mic, MicError> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), MicError>>();
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("pluto-mic".into())
            .spawn(move || run(flag, ready_tx, on_frame))
            .map_err(|e| device_error("thread", e))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Mic {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                Err(MicError::Device("the microphone did not answer".into()))
            }
        }
    }

    fn run(
        stop: Arc<AtomicBool>,
        ready: mpsc::Sender<Result<(), MicError>>,
        on_frame: impl FnMut(Vec<u8>) + Send + 'static,
    ) {
        let built = build(&stop, on_frame);
        match built {
            Ok(stream) => {
                let _ = ready.send(Ok(()));
                while !stop.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(20));
                }
                drop(stream);
            }
            Err(e) => {
                let _ = ready.send(Err(e));
            }
        }
    }

    fn build(
        stop: &Arc<AtomicBool>,
        on_frame: impl FnMut(Vec<u8>) + Send + 'static,
    ) -> Result<cpal::Stream, MicError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| MicError::Device("no microphone found".into()))?;
        let supported = device
            .default_input_config()
            .map_err(|e| device_error("settings", e))?;
        let rate = supported.sample_rate().0;
        let channels = usize::from(supported.channels());
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        /// One chain per stream: samples in, resampled, cut into frames, handed on.
        struct Chain<F> {
            conv: To16kMono,
            framer: Framer,
            on_frame: F,
        }
        impl<F: FnMut(Vec<u8>)> Chain<F> {
            fn feed(&mut self, pcm: &[i16]) {
                for frame in self.framer.push(pcm) {
                    (self.on_frame)(frame);
                }
            }
        }
        let chain = Arc::new(Mutex::new(Chain {
            conv: To16kMono::new(rate, channels),
            framer: Framer::default(),
            on_frame,
        }));
        let on_error = |e: cpal::StreamError| tracing::warn!(error = %e, "microphone stream error");
        let stopped = stop.clone();
        let stream = match format {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut c) = chain.lock() {
                        let pcm = c.conv.push_f32(data);
                        c.feed(&pcm);
                    }
                },
                on_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut c) = chain.lock() {
                        let pcm = c.conv.push_i16(data);
                        c.feed(&pcm);
                    }
                },
                on_error,
                None,
            ),
            other => {
                return Err(MicError::Device(format!(
                    "the microphone's sample format ({other:?}) is not supported"
                )))
            }
        }
        .map_err(|e| device_error("could not open", e))?;
        stream
            .play()
            .map_err(|e| device_error("could not start", e))?;
        Ok(stream)
    }
}

pub use imp::{start, Mic};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_become_whole_32_ms_frames_and_the_rest_waits() {
        let mut f = Framer::default();
        assert!(f.push(&[1; 300]).is_empty());
        let frames = f.push(&[2; 800]);
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().all(|x| x.len() == FRAME_BYTES));
        // 1,100 samples in, 1,024 out, 76 waiting.
        let more = f.push(&[3; 436]);
        assert_eq!(more.len(), 1);
    }

    #[test]
    fn here_it_says_where_it_works() {
        #[cfg(not(any(windows, target_os = "macos")))]
        assert!(matches!(start(|_| {}), Err(MicError::Unsupported)));
    }
}
