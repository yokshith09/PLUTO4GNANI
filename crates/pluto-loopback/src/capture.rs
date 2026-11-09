use thiserror::Error;

/// Why system audio could not be captured. The recording carries on with the microphone alone.
#[derive(Debug, Error)]
pub enum LoopbackError {
    #[error("Capturing the computer's sound is only available on Windows.")]
    Unsupported,
    #[error("Windows would not let Pluto listen to the computer's sound ({0}).")]
    Windows(String),
}

#[cfg(not(windows))]
mod imp {
    use super::LoopbackError;

    /// A running capture. Nothing to stop here.
    pub struct Loopback;

    // Same shape as the Windows handle, where dropping stops the capture.
    impl Drop for Loopback {
        fn drop(&mut self) {}
    }

    /// Starts capturing the computer's sound; `on_pcm` gets 16 kHz mono PCM16 as it arrives.
    pub fn start(_on_pcm: impl FnMut(&[i16]) + Send + 'static) -> Result<Loopback, LoopbackError> {
        Err(LoopbackError::Unsupported)
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    use super::LoopbackError;
    use crate::convert::To16kMono;

    /// How often the buffer is looked at. Loopback has no event to wait on, so it is polled.
    const POLL: Duration = Duration::from_millis(10);
    /// Wave format tags.
    const TAG_PCM: u16 = 1;
    const TAG_FLOAT: u16 = 3;
    const TAG_EXTENSIBLE: u16 = 0xFFFE;

    /// A running capture. Dropping it stops it.
    pub struct Loopback {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for Loopback {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn fail(e: windows::core::Error) -> LoopbackError {
        LoopbackError::Windows(e.message())
    }

    /// Starts capturing the computer's sound; `on_pcm` gets 16 kHz mono PCM16 as it arrives.
    ///
    /// Setup happens on the capture thread (COM must be set up where it is used) and its result is
    /// handed back before this returns, so a failure is an `Err` here and not a silent thread.
    pub fn start(on_pcm: impl FnMut(&[i16]) + Send + 'static) -> Result<Loopback, LoopbackError> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), LoopbackError>>();
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("pluto-loopback".into())
            .spawn(move || run(flag, ready_tx, on_pcm))
            .map_err(|e| LoopbackError::Windows(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Loopback {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(LoopbackError::Windows(
                "the capture thread ended early".into(),
            )),
        }
    }

    /// What the mix format holds.
    #[derive(Clone, Copy)]
    enum Kind {
        Float32,
        Int16,
    }

    struct Format {
        rate: u32,
        channels: usize,
        kind: Kind,
    }

    /// Reads the device's mix format. `fmt` must point at a valid `WAVEFORMATEX`.
    unsafe fn read_format(fmt: *const WAVEFORMATEX) -> Result<Format, LoopbackError> {
        // The structure is packed: read each field without assuming alignment.
        let tag = std::ptr::read_unaligned(std::ptr::addr_of!((*fmt).wFormatTag));
        let channels = std::ptr::read_unaligned(std::ptr::addr_of!((*fmt).nChannels));
        let rate = std::ptr::read_unaligned(std::ptr::addr_of!((*fmt).nSamplesPerSec));
        let bits = std::ptr::read_unaligned(std::ptr::addr_of!((*fmt).wBitsPerSample));
        let align = std::ptr::read_unaligned(std::ptr::addr_of!((*fmt).nBlockAlign));
        // Extensible formats carry the real kind in a GUID whose first four bytes are the tag.
        let effective = if tag == TAG_EXTENSIBLE {
            let ext = fmt.cast::<WAVEFORMATEXTENSIBLE>();
            let sub = std::ptr::read_unaligned(std::ptr::addr_of!((*ext).SubFormat));
            u16::try_from(sub.data1).unwrap_or(0)
        } else {
            tag
        };
        let kind = match (effective, bits) {
            (TAG_FLOAT, 32) => Kind::Float32,
            (TAG_PCM, 16) => Kind::Int16,
            _ => {
                return Err(LoopbackError::Windows(format!(
                    "an audio format Pluto does not read ({bits}-bit, type {effective})"
                )))
            }
        };
        if channels == 0 || rate == 0 || align == 0 {
            return Err(LoopbackError::Windows("an empty audio format".into()));
        }
        Ok(Format {
            rate,
            channels: usize::from(channels),
            kind,
        })
    }

    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            // SAFETY: balances the `CoInitializeEx` made by `run` on this thread.
            unsafe { CoUninitialize() };
        }
    }

    /// Everything that needs the sound system, on its own thread.
    fn run(
        stop: Arc<AtomicBool>,
        ready: mpsc::Sender<Result<(), LoopbackError>>,
        mut on_pcm: impl FnMut(&[i16]),
    ) {
        // SAFETY: COM calls on this thread, in order, with handles the OS gave us.
        let setup = unsafe {
            (|| -> Result<(Com, IAudioClient, IAudioCaptureClient, Format), LoopbackError> {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(fail)?;
                let com = Com;
                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(fail)?;
                // What the person hears: the default output device.
                let device = enumerator
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .map_err(fail)?;
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(fail)?;
                let fmt = client.GetMixFormat().map_err(fail)?;
                let format = read_format(fmt);
                let init = format.as_ref().ok().map(|_| {
                    client.Initialize(
                        AUDCLNT_SHAREMODE_SHARED,
                        AUDCLNT_STREAMFLAGS_LOOPBACK,
                        10_000_000, // a one second buffer, in 100 ns units
                        0,
                        fmt,
                        None,
                    )
                });
                CoTaskMemFree(Some(fmt as *const _));
                let format = format?;
                if let Some(Err(e)) = init {
                    return Err(fail(e));
                }
                let capture: IAudioCaptureClient = client.GetService().map_err(fail)?;
                client.Start().map_err(fail)?;
                Ok((com, client, capture, format))
            })()
        };
        let (_com, client, capture, format) = match setup {
            Ok(parts) => parts,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let _ = ready.send(Ok(()));

        let mut converter = To16kMono::new(format.rate, format.channels);
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(POLL);
            // SAFETY: the capture client belongs to this thread; each buffer is released after it is read.
            let alive = unsafe { drain(&capture, &format, &mut converter, &mut on_pcm) };
            if !alive {
                tracing::warn!(
                    "the computer's sound capture ended (the output device changed or went away)"
                );
                break;
            }
        }
        // SAFETY: stops the client started above.
        let _ = unsafe { client.Stop() };
    }

    /// Reads every waiting packet. `false` when the device is gone.
    unsafe fn drain(
        capture: &IAudioCaptureClient,
        format: &Format,
        converter: &mut To16kMono,
        on_pcm: &mut impl FnMut(&[i16]),
    ) -> bool {
        loop {
            let waiting = match capture.GetNextPacketSize() {
                Ok(n) => n,
                Err(_) => return false,
            };
            if waiting == 0 {
                return true;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            if capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .is_err()
            {
                return false;
            }
            let count = frames as usize;
            let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            let pcm = if silent || data.is_null() {
                // The device says this stretch is silent: its bytes are not to be read.
                match format.kind {
                    Kind::Float32 => converter.push_f32(&vec![0.0; count * format.channels]),
                    Kind::Int16 => converter.push_i16(&vec![0; count * format.channels]),
                }
            } else {
                match format.kind {
                    Kind::Float32 => {
                        let s =
                            std::slice::from_raw_parts(data.cast::<f32>(), count * format.channels);
                        converter.push_f32(s)
                    }
                    Kind::Int16 => {
                        let s =
                            std::slice::from_raw_parts(data.cast::<i16>(), count * format.channels);
                        converter.push_i16(s)
                    }
                }
            };
            if capture.ReleaseBuffer(frames).is_err() {
                return false;
            }
            if !pcm.is_empty() {
                on_pcm(&pcm);
            }
        }
    }
}

pub use imp::{start, Loopback};
