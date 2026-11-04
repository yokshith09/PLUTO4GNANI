//! Meeting intelligence: extraction, validation, dates, recall, recap, recorder, commitments.
//!
//! Owner: **D2**. `extraction`: T-203. Recall, recap and the recorder follow.

pub mod apply;
pub mod extraction;
pub mod service;

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use pluto_contracts::meeting::ExtractionResult;

/// The last extraction of each meeting, held in memory between "Extract" and "Apply" (the review
/// screen sends back only the edited items; the quotes and segments come from here). Lost on
/// restart on purpose: the user extracts again rather than apply a stale review.
#[derive(Default)]
pub struct ExtractionCache(Mutex<HashMap<String, ExtractionResult>>);

impl ExtractionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&self, result: ExtractionResult) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(result.meeting_id.clone(), result);
    }

    pub fn get(&self, meeting_id: &str) -> Option<ExtractionResult> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(meeting_id)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_extraction_per_meeting_wins() {
        let c = ExtractionCache::new();
        let r = |id: &str, lang: &str| ExtractionResult {
            meeting_id: id.into(),
            items: vec![],
            warnings: vec![],
            dominant_language: Some(lang.into()),
        };
        c.put(r("m1", "hi-IN"));
        c.put(r("m1", "ta-IN"));
        c.put(r("m2", "en-IN"));
        assert_eq!(
            c.get("m1").unwrap().dominant_language.as_deref(),
            Some("ta-IN")
        );
        assert!(c.get("m3").is_none());
    }
}
