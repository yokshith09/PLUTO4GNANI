//! Gnani clients (API_CONTRACTS §2.1–§2.5). Every Gnani call in the app goes through this crate.
//!
//! | Module | Owner | Task |
//! |---|---|---|
//! | `http` (request queue, retry, error mapping, privacy log) | D1 | T-103 |
//! | `realtime` (Prisma Realtime WebSocket) | D1 | T-101 |
//! | `tts` (Timbre WebSocket and REST) | D1 | T-102 |
//! | `batch` (Prisma Batch jobs, `meeting_import`) | D1 | T-201 |
//! | `rest` (Prisma REST, comparison) | D2 | T-504 |
//! | `mock` (`MockStt`, `MockTts`; feature `mock`) | D1 | T-004 |

pub mod batch;
pub mod http;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
pub mod realtime;
pub mod rest;
pub mod tts;
