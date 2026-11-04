//! Voice runtime, assistant and privacy (ARCHITECTURE §5, §10, §12). Owner: **D1**.
//!
//! | Module | Task |
//! |---|---|
//! | `audio` (`FrameSink` fan-out, WAV writer) | T-013 |
//! | `runtime` (state machine, push-to-talk, barge-in) | T-301 |
//! | `assistant` (tools, proposals, approval gate, context) | T-303 |
//! | `privacy` (opt-in, usage counters, privacy log) | T-506 |

pub mod assistant;
pub mod audio;
pub mod privacy;
pub mod runtime;
