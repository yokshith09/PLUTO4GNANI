//! What the computer is playing, as audio the recorder can use.
//!
//! On a call the other people's voices come out of the speakers or headphones, not into the
//! microphone. Windows can hand over a copy of whatever the default output device plays (WASAPI
//! "loopback"); this crate captures that copy and converts it to 16 kHz mono 16-bit, the
//! recorder's format. It reads nothing but the output mix and stores nothing itself.
//!
//! Windows only. Elsewhere [`start`] says it is not supported and the recording uses the
//! microphone alone. The conversion in [`convert`] has no operating system in it and is tested
//! everywhere; the capture was checked against the Windows API by the compiler but could not be run
//! outside Windows (see `docs/results/system-audio.md`).

pub mod convert;

mod capture;

pub use capture::{start, Loopback, LoopbackError};
