//! Writes each outbound LLM call to the `privacy_log` table (T-PR-02): where it went, why, how big.
//! Never the content. Fire-and-forget: a slow or failing log must not slow or fail the call.

use std::sync::Arc;

use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::voice_store;
use pluto_gnani::http::{OutboundLog, OutboundRecord};

pub struct DbOutboundLog {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
}

impl DbOutboundLog {
    pub fn new(db: Arc<Database>, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }
}

impl OutboundLog for DbOutboundLog {
    fn record(&self, record: OutboundRecord) {
        // Called from async code; without a runtime there is nothing to spawn onto.
        let Ok(rt) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (db, at) = (self.db.clone(), self.clock.now_ms());
        let destination = serde_json::to_value(record.destination)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        rt.spawn(async move {
            let result = db
                .write(move |tx| {
                    voice_store::log_outbound(
                        tx,
                        at,
                        &destination,
                        &record.purpose,
                        i64::try_from(record.bytes).unwrap_or(i64::MAX),
                        record.ref_id.as_deref(),
                    )?;
                    Ok(((), vec![]))
                })
                .await;
            if result.is_err() {
                tracing::warn!("could not write the privacy log");
            }
        });
    }
}
