//! Store for migration 012 (`meeting_ai`, jobs, transcript segments + FTS): DATA_SCHEMA §2.
//!
//! Owner: **D2** (T-106). Functions take a `Connection` so they run inside `Database::write` /
//! `read` and are tested on an in-memory database. Statuses are stored in their wire spelling
//! (snake_case), so no conversion layer exists.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;

/// The most rows a transcript search returns.
pub const SEARCH_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStatusRow {
    pub meeting_id: String,
    pub status: String,
    pub dominant_language: Option<String>,
    pub updated_at: i64,
}

/// Create or update the pipeline status of a meeting.
pub fn set_status(
    conn: &Connection,
    meeting_id: &str,
    status: &str,
    dominant_language: Option<&str>,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO meeting_ai (meeting_id, status, dominant_language, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(meeting_id) DO UPDATE SET status = ?2,
           dominant_language = COALESCE(?3, dominant_language), updated_at = ?4",
        params![meeting_id, status, dominant_language, now],
    )?;
    Ok(())
}

pub fn get_status(conn: &Connection, meeting_id: &str) -> Result<Option<AiStatusRow>> {
    Ok(conn
        .query_row(
            "SELECT meeting_id, status, dominant_language, updated_at FROM meeting_ai
             WHERE meeting_id = ?1",
            [meeting_id],
            |r| {
                Ok(AiStatusRow {
                    meeting_id: r.get(0)?,
                    status: r.get(1)?,
                    dominant_language: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            },
        )
        .optional()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentRow {
    pub id: String,
    pub meeting_id: String,
    pub index: u32,
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker_label: Option<u8>,
    pub participant_position: Option<u32>,
    pub text: String,
    pub language: Option<String>,
    pub relevance: Option<String>,
}

/// Replace all segments of a meeting (a re-transcription starts clean).
pub fn replace_segments(conn: &Connection, meeting_id: &str, rows: &[SegmentRow]) -> Result<()> {
    conn.execute(
        "DELETE FROM transcript_segments WHERE meeting_id = ?1",
        [meeting_id],
    )?;
    let mut stmt = conn.prepare(
        "INSERT INTO transcript_segments (id, meeting_id, seg_index, start_ms, end_ms,
           speaker_label, participant_position, text, language, relevance)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for r in rows {
        stmt.execute(params![
            r.id,
            meeting_id,
            r.index,
            r.start_ms,
            r.end_ms,
            r.speaker_label,
            r.participant_position,
            r.text,
            r.language,
            r.relevance
        ])?;
    }
    Ok(())
}

fn segment_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<SegmentRow> {
    Ok(SegmentRow {
        id: r.get(0)?,
        meeting_id: r.get(1)?,
        index: r.get(2)?,
        start_ms: r.get(3)?,
        end_ms: r.get(4)?,
        speaker_label: r.get(5)?,
        participant_position: r.get(6)?,
        text: r.get(7)?,
        language: r.get(8)?,
        relevance: r.get(9)?,
    })
}

const SEGMENT_COLS: &str = "s.id, s.meeting_id, s.seg_index, s.start_ms, s.end_ms, s.speaker_label,
  s.participant_position, s.text, s.language, s.relevance";

pub fn list_segments(conn: &Connection, meeting_id: &str) -> Result<Vec<SegmentRow>> {
    let sql = format!(
        "SELECT {SEGMENT_COLS} FROM transcript_segments s WHERE s.meeting_id = ?1 ORDER BY s.seg_index"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([meeting_id], segment_from)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Full-text search over all transcripts. The query is quoted per word, so user text can never
/// be read as FTS syntax. Best matches first.
pub fn search_segments(conn: &Connection, query: &str) -> Result<Vec<SegmentRow>> {
    let fts: String = query
        .split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    if fts.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT {SEGMENT_COLS} FROM transcript_fts f JOIN transcript_segments s ON s.rowid = f.rowid
         WHERE transcript_fts MATCH ?1 ORDER BY rank LIMIT {SEARCH_LIMIT}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([fts], segment_from)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Search every transcript for any of `terms` (best matches first, at most `limit`). Each term is
/// quoted, so user words can never be read as FTS syntax.
pub fn search_segments_any(
    conn: &Connection,
    terms: &[String],
    limit: usize,
) -> Result<Vec<SegmentRow>> {
    let fts: String = terms
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    if fts.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT {SEGMENT_COLS} FROM transcript_fts f JOIN transcript_segments s ON s.rowid = f.rowid
         WHERE transcript_fts MATCH ?1 ORDER BY rank LIMIT {}",
        limit.clamp(1, 50)
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([fts], segment_from)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Record a new transcription attempt.
pub fn insert_job(
    conn: &Connection,
    id: &str,
    meeting_id: &str,
    config_json: &str,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO transcription_jobs (id, meeting_id, status, config_json, created_at, updated_at)
         VALUES (?1, ?2, 'queued', ?3, ?4, ?4)",
        params![id, meeting_id, config_json, now],
    )?;
    Ok(())
}

pub fn update_job(
    conn: &Connection,
    id: &str,
    status: &str,
    gnani_job_id: Option<&str>,
    error: Option<&str>,
    now: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE transcription_jobs SET status = ?2, gnani_job_id = COALESCE(?3, gnani_job_id),
           error = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, status, gnani_job_id, error, now],
    )?;
    Ok(())
}

// ---- provenance: where each approved item came from (T-204) ---------------------------------

/// The segment and words an approved item was taken from, plus what the model said about it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionMeta {
    pub action_item_id: String,
    pub source_segment_id: Option<String>,
    pub quote: String,
    pub due_phrase: Option<String>,
    pub due_date: Option<String>,
    pub priority: Option<String>,
    pub confidence: Option<f64>,
}

/// Record (or replace) the provenance of an action item.
pub fn link_action_item(conn: &Connection, m: &ActionMeta) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO action_item_meta
           (action_item_id, source_segment_id, quote, due_phrase, due_date, priority, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            m.action_item_id,
            m.source_segment_id,
            m.quote,
            m.due_phrase,
            m.due_date,
            m.priority,
            m.confidence
        ],
    )?;
    Ok(())
}

pub fn link_decision(
    conn: &Connection,
    decision_id: &str,
    source_segment_id: Option<&str>,
    quote: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO decision_meta (decision_id, source_segment_id, quote) VALUES (?1, ?2, ?3)",
        params![decision_id, source_segment_id, quote],
    )?;
    Ok(())
}

pub fn get_action_meta(conn: &Connection, action_item_id: &str) -> Result<Option<ActionMeta>> {
    Ok(conn
        .query_row(
            "SELECT action_item_id, source_segment_id, quote, due_phrase, due_date, priority, confidence
             FROM action_item_meta WHERE action_item_id = ?1",
            [action_item_id],
            |r| {
                Ok(ActionMeta {
                    action_item_id: r.get(0)?,
                    source_segment_id: r.get(1)?,
                    quote: r.get(2)?,
                    due_phrase: r.get(3)?,
                    due_date: r.get(4)?,
                    priority: r.get(5)?,
                    confidence: r.get(6)?,
                })
            },
        )
        .optional()?)
}

/// The action item of this meeting that already came from this segment (so applying twice reuses
/// it instead of adding a copy). If a segment produced several, the first by position.
pub fn find_action_item_from(
    conn: &Connection,
    meeting_id: &str,
    segment_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT a.id FROM meeting_action_items a JOIN action_item_meta m ON m.action_item_id = a.id
         WHERE a.meeting_id = ?1 AND m.source_segment_id = ?2 ORDER BY a.position",
    )?;
    let ids = stmt
        .query_map(params![meeting_id, segment_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

pub fn find_decision_from(
    conn: &Connection,
    meeting_id: &str,
    segment_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT d.id FROM meeting_decisions d JOIN decision_meta m ON m.decision_id = d.id
         WHERE d.meeting_id = ?1 AND m.source_segment_id = ?2 ORDER BY d.position",
    )?;
    let ids = stmt
        .query_map(params![meeting_id, segment_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsightRow {
    pub id: String,
    pub kind: String,
    pub body: String,
    pub source_segment_id: Option<String>,
}

/// Replace a meeting's open questions and risks (the base has no table for them).
pub fn set_insights(
    conn: &Connection,
    meeting_id: &str,
    rows: &[InsightRow],
    now: i64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM meeting_insights WHERE meeting_id = ?1",
        [meeting_id],
    )?;
    for r in rows {
        conn.execute(
            "INSERT INTO meeting_insights (id, meeting_id, kind, body, source_segment_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![r.id, meeting_id, r.kind, r.body, r.source_segment_id, now],
        )?;
    }
    Ok(())
}

pub fn list_insights(conn: &Connection, meeting_id: &str) -> Result<Vec<InsightRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, body, source_segment_id FROM meeting_insights WHERE meeting_id = ?1 ORDER BY created_at, rowid",
    )?;
    let rows = stmt
        .query_map([meeting_id], |r| {
            Ok(InsightRow {
                id: r.get(0)?,
                kind: r.get(1)?,
                body: r.get(2)?,
                source_segment_id: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Name each speaker: `position` is the index into the meeting's participant list (`None` clears it).
pub fn set_speaker_positions(
    conn: &Connection,
    meeting_id: &str,
    mapping: &[(u8, Option<u32>)],
) -> Result<()> {
    for (label, position) in mapping {
        conn.execute(
            "UPDATE transcript_segments SET participant_position = ?3 WHERE meeting_id = ?1 AND speaker_label = ?2",
            params![meeting_id, label, position],
        )?;
    }
    Ok(())
}

/// An applied decision or action item with where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedRow {
    pub id: String,
    pub body: String,
    pub assignee: Option<String>,
    pub meta: Option<ActionMeta>,
    /// For decisions: the segment and quote.
    pub decision_source: Option<(Option<String>, String)>,
}

pub fn applied_actions(conn: &Connection, meeting_id: &str) -> Result<Vec<AppliedRow>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.body, a.assignee FROM meeting_action_items a
         JOIN action_item_meta m ON m.action_item_id = a.id WHERE a.meeting_id = ?1 ORDER BY a.position",
    )?;
    let base: Vec<(String, String, Option<String>)> = stmt
        .query_map([meeting_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    base.into_iter()
        .map(|(id, body, assignee)| {
            let meta = get_action_meta(conn, &id)?;
            Ok(AppliedRow {
                id,
                body,
                assignee,
                meta,
                decision_source: None,
            })
        })
        .collect()
}

pub fn applied_decisions(conn: &Connection, meeting_id: &str) -> Result<Vec<AppliedRow>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.body, m.source_segment_id, m.quote FROM meeting_decisions d
         JOIN decision_meta m ON m.decision_id = d.id WHERE d.meeting_id = ?1 ORDER BY d.position",
    )?;
    let rows = stmt
        .query_map([meeting_id], |r| {
            Ok(AppliedRow {
                id: r.get(0)?,
                body: r.get(1)?,
                assignee: None,
                meta: None,
                decision_source: Some((r.get(2)?, r.get(3)?)),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// One action item with what is known about its task, for the commitments list (T-503).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitmentRow {
    pub action_item_id: String,
    pub meeting_id: String,
    pub meeting_title: String,
    pub owner: Option<String>,
    pub body: String,
    /// The task's due date if there is a task, else the date said in the meeting.
    pub due_date: Option<String>,
    pub task_id: Option<String>,
    pub task_status: Option<String>,
}

/// Every action item of every meeting, newest meeting first.
pub fn commitment_rows(conn: &Connection) -> Result<Vec<CommitmentRow>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.meeting_id, m.title, a.assignee, a.body, COALESCE(t.due_date, mt.due_date), t.id, t.status
         FROM meeting_action_items a
         JOIN meetings m ON m.id = a.meeting_id
         LEFT JOIN tasks t ON t.id = a.task_id
         LEFT JOIN action_item_meta mt ON mt.action_item_id = a.id
         ORDER BY m.starts_at DESC, a.position",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CommitmentRow {
                action_item_id: r.get(0)?,
                meeting_id: r.get(1)?,
                meeting_title: r.get(2)?,
                owner: r.get(3)?,
                body: r.get(4)?,
                due_date: r.get(5)?,
                task_id: r.get(6)?,
                task_status: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    pub id: String,
    pub meeting_id: String,
    pub gnani_job_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub updated_at: i64,
}

/// The newest transcription attempt of a meeting, if it has any (a hand-typed meeting has none).
pub fn latest_job(conn: &Connection, meeting_id: &str) -> Result<Option<JobRow>> {
    Ok(conn
        .query_row(
            "SELECT id, meeting_id, gnani_job_id, status, error, updated_at FROM transcription_jobs
             WHERE meeting_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [meeting_id],
            |r| {
                Ok(JobRow {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    gnani_job_id: r.get(2)?,
                    status: r.get(3)?,
                    error: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrate, open_memory, MIGRATIONS};

    fn db() -> Connection {
        let mut conn = open_memory().unwrap();
        migrate::migrate(&mut conn, MIGRATIONS).unwrap();
        conn.execute(
            "INSERT INTO meetings (id, title, starts_at, created_at, updated_at)
             VALUES ('m1', 'Sync', 0, 0, 0)",
            [],
        )
        .unwrap();
        conn
    }

    fn seg(id: &str, index: u32, text: &str) -> SegmentRow {
        SegmentRow {
            id: id.into(),
            meeting_id: "m1".into(),
            index,
            start_ms: i64::from(index) * 1000,
            end_ms: i64::from(index) * 1000 + 900,
            speaker_label: Some(1),
            participant_position: None,
            text: text.into(),
            language: Some("hi-IN".into()),
            relevance: None,
        }
    }

    #[test]
    fn status_is_created_then_updated() {
        let c = db();
        set_status(&c, "m1", "transcribing", None, 10).unwrap();
        set_status(&c, "m1", "done", Some("hi-IN"), 20).unwrap();
        let s = get_status(&c, "m1").unwrap().unwrap();
        assert_eq!((s.status.as_str(), s.updated_at), ("done", 20));
        assert_eq!(s.dominant_language.as_deref(), Some("hi-IN"));
        assert!(set_status(&c, "m1", "bogus", None, 1).is_err());
    }

    #[test]
    fn segments_round_trip_in_order_and_are_replaced() {
        let c = db();
        replace_segments(&c, "m1", &[seg("b", 1, "second"), seg("a", 0, "first")]).unwrap();
        let ids: Vec<_> = list_segments(&c, "m1")
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["a", "b"]);
        replace_segments(&c, "m1", &[seg("c", 0, "only")]).unwrap();
        assert_eq!(list_segments(&c, "m1").unwrap().len(), 1);
        assert!(search_segments(&c, "first").unwrap().is_empty());
    }

    #[test]
    fn search_finds_words_and_ignores_fts_syntax() {
        let c = db();
        replace_segments(
            &c,
            "m1",
            &[
                seg("a", 0, "ship the budget on Friday"),
                seg("b", 1, "lunch"),
            ],
        )
        .unwrap();
        assert_eq!(search_segments(&c, "budget friday").unwrap()[0].id, "a");
        assert!(search_segments(&c, "\" OR NEAR(").is_ok());
        assert!(search_segments(&c, "   ").unwrap().is_empty());
    }

    #[test]
    fn deleting_a_meeting_removes_its_ai_data() {
        let c = db();
        set_status(&c, "m1", "done", None, 1).unwrap();
        replace_segments(&c, "m1", &[seg("a", 0, "hello")]).unwrap();
        insert_job(&c, "j1", "m1", "{}", 1).unwrap();
        c.execute("DELETE FROM meetings WHERE id = 'm1'", [])
            .unwrap();
        assert!(get_status(&c, "m1").unwrap().is_none());
        assert!(search_segments(&c, "hello").unwrap().is_empty());
        let n: i64 = c
            .query_row("SELECT count(*) FROM transcription_jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn job_lifecycle_keeps_the_gnani_id() {
        let c = db();
        insert_job(&c, "j1", "m1", "{}", 1).unwrap();
        update_job(&c, "j1", "created", Some("g-1"), None, 2).unwrap();
        update_job(&c, "j1", "failed", None, Some("boom"), 3).unwrap();
        let (id, st, err): (String, String, Option<String>) = c
            .query_row(
                "SELECT gnani_job_id, status, error FROM transcription_jobs",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (id.as_str(), st.as_str(), err.as_deref()),
            ("g-1", "failed", Some("boom"))
        );
    }

    fn seed_item(c: &Connection) {
        replace_segments(c, "m1", &[seg("s1", 0, "I will send the report Friday")]).unwrap();
        c.execute("INSERT INTO meeting_action_items (id, meeting_id, position, body) VALUES ('a1','m1',0,'Send the report')", []).unwrap();
        c.execute("INSERT INTO meeting_decisions (id, meeting_id, position, body) VALUES ('d1','m1',0,'Ship on Friday')", []).unwrap();
    }

    #[test]
    fn provenance_is_stored_replaced_and_found_again() {
        let c = db();
        seed_item(&c);
        let meta = ActionMeta {
            action_item_id: "a1".into(),
            source_segment_id: Some("s1".into()),
            quote: "send the report Friday".into(),
            due_phrase: Some("Friday".into()),
            due_date: Some("2026-11-06".into()),
            priority: Some("HIGH".into()),
            confidence: None,
        };
        link_action_item(&c, &meta).unwrap();
        link_decision(&c, "d1", Some("s1"), "Ship on Friday").unwrap();
        assert_eq!(get_action_meta(&c, "a1").unwrap().unwrap(), meta);
        assert_eq!(find_action_item_from(&c, "m1", "s1").unwrap(), ["a1"]);
        assert_eq!(find_decision_from(&c, "m1", "s1").unwrap(), ["d1"]);
        link_action_item(
            &c,
            &ActionMeta {
                quote: "edited".into(),
                ..meta
            },
        )
        .unwrap();
        assert_eq!(
            get_action_meta(&c, "a1").unwrap().unwrap().quote,
            "edited",
            "replaced, not duplicated"
        );
        assert!(
            link_action_item(
                &c,
                &ActionMeta {
                    due_date: Some("Friday".into()),
                    ..get_action_meta(&c, "a1").unwrap().unwrap()
                }
            )
            .is_err(),
            "bad date refused by the schema"
        );
        assert!(find_action_item_from(&c, "m1", "nope").unwrap().is_empty());
    }

    #[test]
    fn insights_are_replaced_as_a_set_and_follow_the_meeting() {
        let c = db();
        replace_segments(&c, "m1", &[seg("s1", 0, "who owns it?")]).unwrap();
        let row = |id: &str, kind: &str| InsightRow {
            id: id.into(),
            kind: kind.into(),
            body: "x".into(),
            source_segment_id: Some("s1".into()),
        };
        set_insights(&c, "m1", &[row("i1", "question"), row("i2", "risk")], 5).unwrap();
        assert_eq!(list_insights(&c, "m1").unwrap().len(), 2);
        set_insights(&c, "m1", &[row("i3", "risk")], 6).unwrap();
        assert_eq!(list_insights(&c, "m1").unwrap(), [row("i3", "risk")]);
        assert!(
            set_insights(&c, "m1", &[row("i4", "chitchat")], 7).is_err(),
            "only question or risk"
        );
        c.execute("DELETE FROM meetings WHERE id = 'm1'", [])
            .unwrap();
        let n: i64 = c
            .query_row("SELECT count(*) FROM meeting_insights", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn deleting_a_segment_keeps_the_item_but_clears_its_source() {
        let c = db();
        seed_item(&c);
        link_decision(&c, "d1", Some("s1"), "q").unwrap();
        replace_segments(&c, "m1", &[]).unwrap();
        let src: Option<String> = c
            .query_row(
                "SELECT source_segment_id FROM decision_meta WHERE decision_id='d1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(src, None);
    }

    #[test]
    fn speakers_are_named_by_position_and_can_be_cleared() {
        let c = db();
        replace_segments(
            &c,
            "m1",
            &[
                seg("s1", 0, "a"),
                SegmentRow {
                    speaker_label: Some(2),
                    ..seg("s2", 1, "b")
                },
            ],
        )
        .unwrap();
        set_speaker_positions(&c, "m1", &[(1, Some(0)), (2, Some(3))]).unwrap();
        let rows = list_segments(&c, "m1").unwrap();
        assert_eq!(
            (rows[0].participant_position, rows[1].participant_position),
            (Some(0), Some(3))
        );
        set_speaker_positions(&c, "m1", &[(1, None)]).unwrap();
        assert_eq!(
            list_segments(&c, "m1").unwrap()[0].participant_position,
            None
        );
    }

    #[test]
    fn applied_rows_come_back_with_their_provenance() {
        let c = db();
        seed_item(&c);
        link_action_item(
            &c,
            &ActionMeta {
                action_item_id: "a1".into(),
                source_segment_id: Some("s1".into()),
                quote: "q".into(),
                due_phrase: None,
                due_date: None,
                priority: None,
                confidence: None,
            },
        )
        .unwrap();
        link_decision(&c, "d1", Some("s1"), "dq").unwrap();
        let a = applied_actions(&c, "m1").unwrap();
        assert_eq!(
            (a[0].id.as_str(), a[0].meta.as_ref().unwrap().quote.as_str()),
            ("a1", "q")
        );
        let d = applied_decisions(&c, "m1").unwrap();
        assert_eq!(d[0].decision_source, Some((Some("s1".into()), "dq".into())));
        assert!(applied_actions(&c, "other").unwrap().is_empty());
    }

    #[test]
    fn any_term_search_ranks_and_ignores_syntax() {
        let c = db();
        replace_segments(
            &c,
            "m1",
            &[
                seg("a", 0, "launch date is 20 November"),
                seg("b", 1, "budget approved"),
                seg("c", 2, "lunch plans"),
            ],
        )
        .unwrap();
        let hits = search_segments_any(&c, &["launch".into(), "budget".into()], 8).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(search_segments_any(&c, &["\" OR NEAR(".into()], 8).is_ok());
        assert!(search_segments_any(&c, &[], 8).unwrap().is_empty());
        assert_eq!(
            search_segments_any(&c, &["launch".into(), "budget".into()], 1)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn commitment_rows_join_the_task_and_prefer_its_due_date() {
        let c = db();
        c.execute("INSERT INTO meeting_action_items (id, meeting_id, position, body, assignee) VALUES ('a1','m1',0,'Send report','Ravi'), ('a2','m1',1,'Book room',NULL)", []).unwrap();
        replace_segments(&c, "m1", &[seg("s1", 0, "x")]).unwrap();
        link_action_item(
            &c,
            &ActionMeta {
                action_item_id: "a1".into(),
                source_segment_id: Some("s1".into()),
                quote: "q".into(),
                due_phrase: None,
                due_date: Some("2026-11-05".into()),
                priority: None,
                confidence: None,
            },
        )
        .unwrap();
        c.execute("INSERT INTO tasks (id, title, status, due_date, created_at, updated_at) VALUES ('t1','Send report','PLANNED','2026-11-09',0,0)", []).unwrap();
        c.execute(
            "UPDATE meeting_action_items SET task_id = 't1' WHERE id = 'a1'",
            [],
        )
        .unwrap();
        let rows = commitment_rows(&c).unwrap();
        assert_eq!(rows.len(), 2);
        let a1 = rows.iter().find(|r| r.action_item_id == "a1").unwrap();
        assert_eq!(
            (
                a1.due_date.as_deref(),
                a1.task_status.as_deref(),
                a1.owner.as_deref()
            ),
            (Some("2026-11-09"), Some("PLANNED"), Some("Ravi"))
        );
        let a2 = rows.iter().find(|r| r.action_item_id == "a2").unwrap();
        assert_eq!(
            (a2.due_date.as_deref(), a2.task_id.as_deref()),
            (None, None)
        );
        assert_eq!(a2.meeting_title, "Sync");
    }

    #[test]
    fn the_latest_job_is_the_newest_and_typed_meetings_have_none() {
        let c = db();
        assert!(latest_job(&c, "m1").unwrap().is_none());
        insert_job(&c, "j1", "m1", "{}", 1).unwrap();
        insert_job(&c, "j2", "m1", "{}", 2).unwrap();
        update_job(&c, "j2", "completed", Some("g-2"), None, 3).unwrap();
        let j = latest_job(&c, "m1").unwrap().unwrap();
        assert_eq!(
            (
                j.id.as_str(),
                j.status.as_str(),
                j.gnani_job_id.as_deref(),
                j.updated_at
            ),
            ("j2", "completed", Some("g-2"), 3)
        );
    }
}
