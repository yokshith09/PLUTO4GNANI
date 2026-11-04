//! Shared types and traits between `pluto-voice` and `pluto-intel` (API_CONTRACTS §3).
//!
//! No logic lives here. `pluto-voice` and `pluto-intel` never depend on each other; they talk only
//! through the traits in this crate, and `src-tauri` wires the implementations together.
//!
//! **This file is written once (T-003) and not edited again** (WORKING_AGREEMENT §2), except for
//! the `CONTRACT_VERSION` bump that every contract PR makes (§5). Put new items in the module files.
//!
//! | Module | Owner |
//! |---|---|
//! | `voice`, `speech`, `error` | D1 |
//! | `meeting`, `llm`, `decide` | D2 |

pub mod decide;
pub mod error;
pub mod llm;
pub mod meeting;
/// Test doubles for the D1 traits. Feature `mock`; `MockStt` / `MockTts` are in `pluto-gnani::mock`.
#[cfg(any(test, feature = "mock"))]
pub mod mock;
pub mod speech;
pub mod voice;

/// Bumped by every contract PR, together with the version line in `API_CONTRACTS.md`.
/// Every fixture in `test-data/fixtures/` carries the same number (T-CT-05).
pub const CONTRACT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    #[test]
    fn contract_version_matches_api_contracts_md() {
        assert_eq!(super::CONTRACT_VERSION, 1);
    }
}
