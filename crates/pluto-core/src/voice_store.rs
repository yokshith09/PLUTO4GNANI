//! Store for migration 013 (`voice_interactions`, `voice_proposals`, `privacy_log`): DATA_SCHEMA §3.
//!
//! Owner: **D1** (T-107). Functions take a `Connection`, so they run inside `Database::write` /
//! `read` and are tested on an in-memory database. Enum-like columns hold their wire spelling
//! (snake_case); the table CHECKs reject anything else. `pluto-core` cannot depend on
//! `pluto-contracts`, so the typed enums live there and `pluto-voice` maps them to these strings.
//!
//! **The proposal gate.** [`decide_proposal`] is the only way out of `pending`: one conditional
//! `UPDATE`, so a proposal is approved at most once and never after `expires_at`. A write tool runs
//! only after it returns [`Decided::Approved`] (`pluto-voice::assistant`, T-AS-05).

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::Result;

/// The most rows a list call returns.
pub const MAX_LIST: u32 = 500;

pub const DECISION_SOURCES: [&str; 2] = ["rules", "llm"];
pub const LLM_PROVIDERS: [&str; 2] = ["gemini", "groq"];
pub const OUTCOMES: [&str; 6] = [
    "answered",
    "executed",
    "rejected",
    "expired",
    "failed",
    "cancelled",
];
pub const PROPOSAL_STATUSES: [&str; 5] = ["pending", "approved", "rejected", "expired", "failed"];
pub const DESTINATIONS: [&str; 7] = [
    "gnani_realtime",
    "gnani_rest",
    "gnani_batch",
    "gnani_tts",
    "gnani_clone",
    "gemini",
    "groq",
];

fn clamp(limit: u32) -> u32 {
    limit.clamp(1, MAX_LIST)
}

// ---- interactions --------------------------------------------------------------------------

/// One assistant turn (`voice_interactions`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractionRow {
    pub id: String,
    pub started_at: i64,
    /// What the user said. Never logged above `debug` (WORKING_AGREEMENT §6).
    pub transcript: String,
    pub language: Option<String>,
    pub intent: Option<String>,
    /// `rules` | `llm`.
    pub decision_source: Option<String>,
    /// `gemini` | `groq`.
    pub llm_provider: Option<String>,
    pub tool: Option<String>,
    pub args_json: Option<String>,
    /// `answered` | `executed` | `rejected` | `expired` | `failed` | `cancelled`.
    pub outcome: Option<String>,
    pub stt_latency_ms: Option<i64>,
    pub llm_latency_ms: Option<i64>,
    pub tts_first_audio_ms: Option<i64>,
}

/// Fields filled in as a turn progresses. `None` leaves a column unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractionUpdate {
    pub intent: Option<String>,
    pub decision_source: Option<String>,
    pub llm_provider: Option<String>,
    pub tool: Option<String>,
    pub args_json: Option<String>,
    pub outcome: Option<String>,
    pub llm_latency_ms: Option<i64>,
    pub tts_first_audio_ms: Option<i64>,
}

const INTERACTION_COLUMNS: &str = "id, started_at, transcript, language, intent, decision_source, \
     llm_provider, tool, args_json, outcome, stt_latency_ms, llm_latency_ms, tts_first_audio_ms";

fn interaction_from(r: &Row<'_>) -> rusqlite::Result<InteractionRow> {
    Ok(InteractionRow {
        id: r.get(0)?,
        started_at: r.get(1)?,
        transcript: r.get(2)?,
        language: r.get(3)?,
        intent: r.get(4)?,
        decision_source: r.get(5)?,
        llm_provider: r.get(6)?,
        tool: r.get(7)?,
        args_json: r.get(8)?,
        outcome: r.get(9)?,
        stt_latency_ms: r.get(10)?,
        llm_latency_ms: r.get(11)?,
        tts_first_audio_ms: r.get(12)?,
    })
}

pub fn insert_interaction(conn: &Connection, row: &InteractionRow) -> Result<()> {
    conn.execute(
        &format!("INSERT INTO voice_interactions ({INTERACTION_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"),
        params![
            row.id,
            row.started_at,
            row.transcript,
            row.language,
            row.intent,
            row.decision_source,
            row.llm_provider,
            row.tool,
            row.args_json,
            row.outcome,
            row.stt_latency_ms,
            row.llm_latency_ms,
            row.tts_first_audio_ms,
        ],
    )?;
    Ok(())
}

/// Applies the set fields of `update`. Returns `false` if the interaction does not exist.
pub fn update_interaction(conn: &Connection, id: &str, update: &InteractionUpdate) -> Result<bool> {
    let n = conn.execute(
        "UPDATE voice_interactions SET
           intent = COALESCE(?2, intent),
           decision_source = COALESCE(?3, decision_source),
           llm_provider = COALESCE(?4, llm_provider),
           tool = COALESCE(?5, tool),
           args_json = COALESCE(?6, args_json),
           outcome = COALESCE(?7, outcome),
           llm_latency_ms = COALESCE(?8, llm_latency_ms),
           tts_first_audio_ms = COALESCE(?9, tts_first_audio_ms)
         WHERE id = ?1",
        params![
            id,
            update.intent,
            update.decision_source,
            update.llm_provider,
            update.tool,
            update.args_json,
            update.outcome,
            update.llm_latency_ms,
            update.tts_first_audio_ms,
        ],
    )?;
    Ok(n == 1)
}

pub fn get_interaction(conn: &Connection, id: &str) -> Result<Option<InteractionRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {INTERACTION_COLUMNS} FROM voice_interactions WHERE id = ?1"),
            [id],
            interaction_from,
        )
        .optional()?)
}

/// The newest interactions first (`voice_history`). `limit` is clamped to 1..=[`MAX_LIST`].
pub fn recent_interactions(conn: &Connection, limit: u32) -> Result<Vec<InteractionRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {INTERACTION_COLUMNS} FROM voice_interactions
         ORDER BY started_at DESC, id DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([clamp(limit)], interaction_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Deletes every logged interaction (and, by cascade, its proposals). A proposal still waiting for
/// an answer is kept, with its interaction, so clearing never silently drops a pending write.
pub fn clear_interactions(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM voice_interactions WHERE id NOT IN
           (SELECT interaction_id FROM voice_proposals WHERE status = 'pending')",
        [],
    )?)
}

// ---- proposals -----------------------------------------------------------------------------

/// A proposed write (`voice_proposals`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalRow {
    pub id: String,
    pub interaction_id: String,
    pub tool: String,
    pub args_json: String,
    pub summary_text: String,
    /// `pending` | `approved` | `rejected` | `expired` | `failed`.
    pub status: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub decided_at: Option<i64>,
}

const PROPOSAL_COLUMNS: &str =
    "id, interaction_id, tool, args_json, summary_text, status, created_at, expires_at, decided_at";

fn proposal_from(r: &Row<'_>) -> rusqlite::Result<ProposalRow> {
    Ok(ProposalRow {
        id: r.get(0)?,
        interaction_id: r.get(1)?,
        tool: r.get(2)?,
        args_json: r.get(3)?,
        summary_text: r.get(4)?,
        status: r.get(5)?,
        created_at: r.get(6)?,
        expires_at: r.get(7)?,
        decided_at: r.get(8)?,
    })
}

/// Stores a new proposal. Its `status` must be `pending`; anything else is refused by the gate's
/// own rule (a proposal is born undecided), not just by the CHECK.
pub fn insert_proposal(conn: &Connection, row: &ProposalRow) -> Result<()> {
    if row.status != "pending" || row.decided_at.is_some() {
        return Err(crate::error::AppError::validation(
            "status",
            "A new proposal must be pending.",
        ));
    }
    conn.execute(
        &format!("INSERT INTO voice_proposals ({PROPOSAL_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"),
        params![
            row.id,
            row.interaction_id,
            row.tool,
            row.args_json,
            row.summary_text,
            row.status,
            row.created_at,
            row.expires_at,
            row.decided_at,
        ],
    )?;
    Ok(())
}

pub fn get_proposal(conn: &Connection, id: &str) -> Result<Option<ProposalRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {PROPOSAL_COLUMNS} FROM voice_proposals WHERE id = ?1"),
            [id],
            proposal_from,
        )
        .optional()?)
}

/// The answer the user gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Approve,
    Reject,
}

/// What [`decide_proposal`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// Now `approved`: the caller may run the tool exactly once.
    Approved,
    /// Now `rejected`.
    Rejected,
    /// The window had passed; now `expired`. Nothing may run.
    Expired,
    /// Already decided earlier (the current status). Nothing changed.
    AlreadyDecided(String),
    NotFound,
}

/// The only transition out of `pending`. Approval succeeds only while `now <= expires_at`.
pub fn decide_proposal(conn: &Connection, id: &str, answer: Answer, now: i64) -> Result<Decided> {
    let to = match answer {
        Answer::Approve => "approved",
        Answer::Reject => "rejected",
    };
    // A single conditional UPDATE: two answers racing for the same proposal cannot both win.
    let changed = conn.execute(
        "UPDATE voice_proposals SET status = ?2, decided_at = ?3
         WHERE id = ?1 AND status = 'pending' AND ?3 <= expires_at",
        params![id, to, now],
    )?;
    if changed == 1 {
        return Ok(match answer {
            Answer::Approve => Decided::Approved,
            Answer::Reject => Decided::Rejected,
        });
    }
    let Some(current) = get_proposal(conn, id)? else {
        return Ok(Decided::NotFound);
    };
    if current.status == "pending" {
        // Still pending, so the window has passed.
        conn.execute(
            "UPDATE voice_proposals SET status = 'expired', decided_at = ?2
             WHERE id = ?1 AND status = 'pending'",
            params![id, now],
        )?;
        return Ok(Decided::Expired);
    }
    Ok(Decided::AlreadyDecided(current.status))
}

/// An approved proposal whose tool then failed. Returns `false` unless it was `approved`.
pub fn mark_proposal_failed(conn: &Connection, id: &str) -> Result<bool> {
    let n = conn.execute(
        "UPDATE voice_proposals SET status = 'failed' WHERE id = ?1 AND status = 'approved'",
        [id],
    )?;
    Ok(n == 1)
}

/// Expires every pending proposal whose window passed before `now`. Returns their ids.
pub fn expire_proposals(conn: &Connection, now: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "UPDATE voice_proposals SET status = 'expired', decided_at = ?1
         WHERE status = 'pending' AND expires_at < ?1 RETURNING id",
    )?;
    let ids = stmt
        .query_map([now], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

/// Pending proposals, oldest first.
pub fn pending_proposals(conn: &Connection) -> Result<Vec<ProposalRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PROPOSAL_COLUMNS} FROM voice_proposals WHERE status = 'pending'
         ORDER BY created_at, id"
    ))?;
    let rows = stmt
        .query_map([], proposal_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---- privacy log ---------------------------------------------------------------------------

/// One outbound request (`privacy_log`). `purpose` says why, never what (no content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyRow {
    pub id: i64,
    pub at: i64,
    /// One of [`DESTINATIONS`].
    pub destination: String,
    pub purpose: String,
    pub bytes: i64,
    pub ref_id: Option<String>,
}

/// Records an outbound request (T-PR-02). Returns its id.
pub fn log_outbound(
    conn: &Connection,
    at: i64,
    destination: &str,
    purpose: &str,
    bytes: i64,
    ref_id: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO privacy_log (at, destination, purpose, bytes, ref_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![at, destination, purpose, bytes, ref_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The newest entries first. `limit` is clamped to 1..=[`MAX_LIST`].
pub fn privacy_log_list(conn: &Connection, limit: u32) -> Result<Vec<PrivacyRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, at, destination, purpose, bytes, ref_id FROM privacy_log
         ORDER BY at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([clamp(limit)], |r| {
            Ok(PrivacyRow {
                id: r.get(0)?,
                at: r.get(1)?,
                destination: r.get(2)?,
                purpose: r.get(3)?,
                bytes: r.get(4)?,
                ref_id: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Deletes the whole log. Returns how many entries were removed.
pub fn privacy_log_clear(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM privacy_log", [])?)
}

/// Requests and bytes sent to `destination` at or after `since` (usage counters, T-506).
pub fn outbound_since(conn: &Connection, destination: &str, since: i64) -> Result<(i64, i64)> {
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(bytes), 0) FROM privacy_log WHERE destination = ?1 AND at >= ?2",
        params![destination, since],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate::{self, MIGRATIONS};
    use crate::db::open_memory;

    fn db() -> Connection {
        let mut conn = open_memory().unwrap();
        migrate::migrate(&mut conn, MIGRATIONS).unwrap();
        conn
    }

    fn interaction(id: &str, at: i64) -> InteractionRow {
        InteractionRow {
            id: id.into(),
            started_at: at,
            transcript: "kal 5 baje DBMS submit karne ka reminder lagao".into(),
            language: Some("hi-IN".into()),
            stt_latency_ms: Some(420),
            ..InteractionRow::default()
        }
    }

    fn proposal(id: &str, interaction_id: &str, expires_at: i64) -> ProposalRow {
        ProposalRow {
            id: id.into(),
            interaction_id: interaction_id.into(),
            tool: "reminder_create".into(),
            args_json: r#"{"title":"DBMS submit","remind_at":1}"#.into(),
            summary_text: "Reminder 'DBMS submit' for 5 PM tomorrow. Shall I set it?".into(),
            status: "pending".into(),
            created_at: 1_000,
            expires_at,
            decided_at: None,
        }
    }

    fn exec(conn: &Connection, sql: &str) -> rusqlite::Result<usize> {
        conn.execute(sql, [])
    }

    // ---- T-DB-05: the constraints hold ----

    #[test]
    fn interaction_enums_are_checked() {
        let c = db();
        let ok = "INSERT INTO voice_interactions (id, started_at, transcript, decision_source, llm_provider, outcome)";
        assert!(exec(
            &c,
            &format!("{ok} VALUES ('a', 1, 't', 'rules', 'groq', 'answered')")
        )
        .is_ok());
        assert!(exec(&c, &format!("{ok} VALUES ('b', 1, 't', NULL, NULL, NULL)")).is_ok());
        assert!(exec(
            &c,
            &format!("{ok} VALUES ('c', 1, 't', 'regex', NULL, NULL)")
        )
        .is_err());
        assert!(exec(
            &c,
            &format!("{ok} VALUES ('d', 1, 't', NULL, 'openai', NULL)")
        )
        .is_err());
        assert!(exec(
            &c,
            &format!("{ok} VALUES ('e', 1, 't', NULL, NULL, 'done')")
        )
        .is_err());
        assert!(
            exec(
                &c,
                &format!("{ok} VALUES ('f', 1, 't', 'Rules', NULL, NULL)")
            )
            .is_err(),
            "wire spelling only"
        );
        assert!(
            exec(
                &c,
                "INSERT INTO voice_interactions (id, started_at) VALUES ('g', 1)"
            )
            .is_err(),
            "transcript required"
        );
    }

    #[test]
    fn every_listed_value_is_accepted_by_the_checks() {
        let c = db();
        for (i, s) in DECISION_SOURCES.iter().enumerate() {
            exec(&c, &format!("INSERT INTO voice_interactions (id, started_at, transcript, decision_source) VALUES ('ds{i}', 1, 't', '{s}')")).unwrap();
        }
        for (i, p) in LLM_PROVIDERS.iter().enumerate() {
            exec(&c, &format!("INSERT INTO voice_interactions (id, started_at, transcript, llm_provider) VALUES ('lp{i}', 1, 't', '{p}')")).unwrap();
        }
        for (i, o) in OUTCOMES.iter().enumerate() {
            exec(&c, &format!("INSERT INTO voice_interactions (id, started_at, transcript, outcome) VALUES ('o{i}', 1, 't', '{o}')")).unwrap();
        }
        for (i, s) in PROPOSAL_STATUSES.iter().enumerate() {
            exec(&c, &format!("INSERT INTO voice_proposals (id, interaction_id, tool, args_json, summary_text, status, created_at, expires_at) VALUES ('p{i}', 'o0', 't', '{{}}', 's', '{s}', 1, 2)")).unwrap();
        }
        for d in DESTINATIONS {
            log_outbound(&c, 1, d, "test", 0, None).unwrap();
        }
    }

    #[test]
    fn proposal_status_and_parent_are_checked() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        let cols = "INSERT INTO voice_proposals (id, interaction_id, tool, args_json, summary_text, status, created_at, expires_at)";
        assert!(exec(
            &c,
            &format!("{cols} VALUES ('p1', 'i1', 't', '{{}}', 's', 'pending', 1, 2)")
        )
        .is_ok());
        assert!(exec(
            &c,
            &format!("{cols} VALUES ('p2', 'i1', 't', '{{}}', 's', 'maybe', 1, 2)")
        )
        .is_err());
        assert!(
            exec(
                &c,
                &format!("{cols} VALUES ('p3', 'nobody', 't', '{{}}', 's', 'pending', 1, 2)")
            )
            .is_err(),
            "a proposal needs its interaction"
        );
        assert!(exec(&c, "INSERT INTO voice_proposals (id, interaction_id, tool, summary_text, status, created_at, expires_at) VALUES ('p4', 'i1', 't', 's', 'pending', 1, 2)").is_err(), "args required");
    }

    #[test]
    fn deleting_an_interaction_deletes_its_proposals() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        insert_proposal(&c, &proposal("p1", "i1", 31_000)).unwrap();
        exec(&c, "DELETE FROM voice_interactions WHERE id = 'i1'").unwrap();
        assert!(get_proposal(&c, "p1").unwrap().is_none());
    }

    #[test]
    fn privacy_destination_and_types_are_checked() {
        let c = db();
        assert!(log_outbound(&c, 1, "gnani_tts", "assistant reply", 120, None).is_ok());
        assert!(log_outbound(&c, 1, "openai", "x", 1, None).is_err());
        assert!(log_outbound(&c, 1, "Gemini", "x", 1, None).is_err());
        assert!(
            exec(&c, "INSERT INTO privacy_log (at, destination, purpose, bytes) VALUES (1, 'groq', 'x', 'lots')").is_err(),
            "STRICT: bytes is an integer"
        );
    }

    // ---- interactions ----

    #[test]
    fn an_interaction_is_filled_in_as_the_turn_progresses() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 10)).unwrap();
        let updated = update_interaction(
            &c,
            "i1",
            &InteractionUpdate {
                intent: Some("reminder".into()),
                decision_source: Some("llm".into()),
                llm_provider: Some("groq".into()),
                tool: Some("reminder_create".into()),
                llm_latency_ms: Some(900),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(updated);
        update_interaction(
            &c,
            "i1",
            &InteractionUpdate {
                outcome: Some("executed".into()),
                tts_first_audio_ms: Some(1_800),
                ..Default::default()
            },
        )
        .unwrap();
        let row = get_interaction(&c, "i1").unwrap().unwrap();
        assert_eq!(
            row.intent.as_deref(),
            Some("reminder"),
            "kept from the first update"
        );
        assert_eq!(row.outcome.as_deref(), Some("executed"));
        assert_eq!(row.llm_latency_ms, Some(900));
        assert_eq!(row.tts_first_audio_ms, Some(1_800));
        assert_eq!(row.stt_latency_ms, Some(420));
        assert!(!update_interaction(&c, "nope", &InteractionUpdate::default()).unwrap());
        assert!(update_interaction(
            &c,
            "i1",
            &InteractionUpdate {
                outcome: Some("finished".into()),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn history_is_newest_first_and_limited() {
        let c = db();
        for (id, at) in [("a", 1), ("b", 3), ("c", 2)] {
            insert_interaction(&c, &interaction(id, at)).unwrap();
        }
        let ids: Vec<_> = recent_interactions(&c, 10)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, ["b", "c", "a"]);
        assert_eq!(recent_interactions(&c, 2).unwrap().len(), 2);
        assert_eq!(
            recent_interactions(&c, 0).unwrap().len(),
            1,
            "0 is clamped to 1"
        );
    }

    // ---- the proposal gate ----

    #[test]
    fn a_new_proposal_must_be_pending() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        let mut p = proposal("p1", "i1", 31_000);
        p.status = "approved".into();
        assert!(insert_proposal(&c, &p).is_err());
        p.status = "pending".into();
        p.decided_at = Some(5);
        assert!(insert_proposal(&c, &p).is_err());
        assert!(get_proposal(&c, "p1").unwrap().is_none());
    }

    #[test]
    fn approval_inside_the_window_happens_exactly_once() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        insert_proposal(&c, &proposal("p1", "i1", 31_000)).unwrap();
        assert_eq!(
            decide_proposal(&c, "p1", Answer::Approve, 20_000).unwrap(),
            Decided::Approved
        );
        assert_eq!(
            decide_proposal(&c, "p1", Answer::Approve, 21_000).unwrap(),
            Decided::AlreadyDecided("approved".into()),
            "a second approval does nothing"
        );
        assert_eq!(
            decide_proposal(&c, "p1", Answer::Reject, 21_000).unwrap(),
            Decided::AlreadyDecided("approved".into())
        );
        let p = get_proposal(&c, "p1").unwrap().unwrap();
        assert_eq!(
            (p.status.as_str(), p.decided_at),
            ("approved", Some(20_000))
        );
    }

    #[test]
    fn approval_on_the_last_millisecond_counts_but_one_later_expires() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        insert_proposal(&c, &proposal("edge", "i1", 31_000)).unwrap();
        insert_proposal(&c, &proposal("late", "i1", 31_000)).unwrap();
        assert_eq!(
            decide_proposal(&c, "edge", Answer::Approve, 31_000).unwrap(),
            Decided::Approved
        );
        assert_eq!(
            decide_proposal(&c, "late", Answer::Approve, 31_001).unwrap(),
            Decided::Expired
        );
        let late = get_proposal(&c, "late").unwrap().unwrap();
        assert_eq!(
            (late.status.as_str(), late.decided_at),
            ("expired", Some(31_001))
        );
        assert_eq!(
            decide_proposal(&c, "late", Answer::Approve, 31_002).unwrap(),
            Decided::AlreadyDecided("expired".into())
        );
    }

    #[test]
    fn rejection_and_unknown_ids() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        insert_proposal(&c, &proposal("p1", "i1", 31_000)).unwrap();
        assert_eq!(
            decide_proposal(&c, "p1", Answer::Reject, 2_000).unwrap(),
            Decided::Rejected
        );
        assert_eq!(
            decide_proposal(&c, "nope", Answer::Approve, 2_000).unwrap(),
            Decided::NotFound
        );
    }

    #[test]
    fn only_an_approved_proposal_can_be_marked_failed() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        insert_proposal(&c, &proposal("p1", "i1", 31_000)).unwrap();
        assert!(!mark_proposal_failed(&c, "p1").unwrap(), "still pending");
        decide_proposal(&c, "p1", Answer::Approve, 2_000).unwrap();
        assert!(mark_proposal_failed(&c, "p1").unwrap());
        assert_eq!(get_proposal(&c, "p1").unwrap().unwrap().status, "failed");
        assert!(!mark_proposal_failed(&c, "p1").unwrap());
    }

    #[test]
    fn expiry_sweeps_only_overdue_pending_proposals() {
        let c = db();
        insert_interaction(&c, &interaction("i1", 1)).unwrap();
        insert_proposal(&c, &proposal("old", "i1", 10_000)).unwrap();
        insert_proposal(&c, &proposal("fresh", "i1", 50_000)).unwrap();
        insert_proposal(&c, &proposal("done", "i1", 10_000)).unwrap();
        decide_proposal(&c, "done", Answer::Reject, 5_000).unwrap();
        assert_eq!(expire_proposals(&c, 20_000).unwrap(), ["old"]);
        let pending: Vec<_> = pending_proposals(&c)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(pending, ["fresh"]);
        assert_eq!(
            get_proposal(&c, "done").unwrap().unwrap().status,
            "rejected"
        );
        assert!(expire_proposals(&c, 20_000).unwrap().is_empty());
    }

    // ---- privacy log ----

    #[test]
    fn the_privacy_log_lists_newest_first_counts_and_clears() {
        let c = db();
        let first = log_outbound(
            &c,
            100,
            "gnani_realtime",
            "assistant command",
            32_000,
            Some("rq-1"),
        )
        .unwrap();
        log_outbound(&c, 300, "groq", "intent", 900, None).unwrap();
        log_outbound(&c, 200, "groq", "intent", 800, None).unwrap();
        let rows = privacy_log_list(&c, 10).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.at).collect::<Vec<_>>(),
            [300, 200, 100]
        );
        let oldest = &rows[2];
        assert_eq!(oldest.id, first);
        assert_eq!(oldest.ref_id.as_deref(), Some("rq-1"));
        assert_eq!(outbound_since(&c, "groq", 0).unwrap(), (2, 1_700));
        assert_eq!(outbound_since(&c, "groq", 250).unwrap(), (1, 900));
        assert_eq!(outbound_since(&c, "gemini", 0).unwrap(), (0, 0));
        assert_eq!(privacy_log_list(&c, 1).unwrap().len(), 1);
        assert_eq!(privacy_log_clear(&c).unwrap(), 3);
        assert!(privacy_log_list(&c, 10).unwrap().is_empty());
    }
}
