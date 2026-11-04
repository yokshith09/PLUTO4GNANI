# Shared fixtures

One set, used by the UI mock (`src/ipc/mockVoice.ts`) and by the Rust test `crates/pluto-contracts/tests/fixtures.rs`.

- Name: `<type>.<case>.json`, in `voice/` or `meetings/` (D2) — `gnani/` is D1's.
- Shape: `{ "contract_version": 1, "data": <the value> }`. `contract_version` equals `CONTRACT_VERSION`.
- Synthetic data only. A fixture follows the documented API shape; if a live response differs, fix the fixture and `API_CONTRACTS.md` in a contract PR.
