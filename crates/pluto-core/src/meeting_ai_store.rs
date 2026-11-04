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
}
