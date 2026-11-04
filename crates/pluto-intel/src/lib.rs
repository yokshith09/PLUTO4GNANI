//! LLM layer, decisions and meeting intelligence (ARCHITECTURE §7–§9). Owner: **D2**.
//!
//! Created empty at T-003; D2 owns this file and the whole crate from here on.
//!
//! | Module | Task |
//! |---|---|
//! | `llm` (Gemini, Groq, router, `MockLlm`) | T-104 |
//! | `decide` (rules → LLM) | T-302 |
//! | `meeting` (extraction, dates, recall, recap, recorder, commitments) | T-203.. |

pub mod dates;
pub mod decide;
pub mod llm;
pub mod meeting;
