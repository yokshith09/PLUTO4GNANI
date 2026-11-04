-- 012: meeting AI — recordings, transcription jobs, transcript segments (+ FTS5), provenance,
-- insights, glossary (DATA_SCHEMA.md §2). Owner: D2 (T-106). The runner owns the transaction.

-- Pipeline status of a meeting's audio/AI processing (1:1 with meetings).
CREATE TABLE meeting_ai (
  meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK (status IN
    ('recording','transcribing','needs_speakers','needs_review','done','failed')),
  dominant_language TEXT,
  updated_at INTEGER NOT NULL
) STRICT;

-- Audio captured or imported for a meeting. Files live in the app data folder.
CREATE TABLE meeting_recordings (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  source TEXT NOT NULL CHECK (source IN ('mic','import','loopback')),
  chunk_index INTEGER NOT NULL,
  offset_ms INTEGER NOT NULL,               -- start of this chunk within the meeting
  file_path TEXT NOT NULL,
  duration_ms INTEGER,
  consent_confirmed_at INTEGER,
  created_at INTEGER NOT NULL,
  CHECK (source = 'import' OR consent_confirmed_at IS NOT NULL)
) STRICT;
CREATE INDEX idx_recordings_meeting ON meeting_recordings (meeting_id, chunk_index);

-- One Gnani Batch job per transcription attempt.
CREATE TABLE transcription_jobs (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  gnani_job_id TEXT,
  status TEXT NOT NULL CHECK (status IN
    ('queued','uploading','created','in_progress','completed','partial_failure','failed','cancelled')),
  config_json TEXT NOT NULL,
  error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_jobs_meeting ON transcription_jobs (meeting_id, created_at DESC);

-- Speaker-labelled segments (Batch result).
CREATE TABLE transcript_segments (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  seg_index INTEGER NOT NULL,               -- our own 0-based order across all chunks
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  speaker_label INTEGER,                    -- Gnani speaker_id as returned
  participant_position INTEGER,             -- set by speaker mapping
  text TEXT NOT NULL,
  language TEXT,
  relevance TEXT CHECK (relevance IS NULL OR relevance IN
    ('decision','action_item','question','risk','chitchat')),
  UNIQUE (meeting_id, seg_index)
) STRICT;
CREATE VIRTUAL TABLE transcript_fts USING fts5(text, content='transcript_segments', content_rowid='rowid');
CREATE TRIGGER transcript_fts_ai AFTER INSERT ON transcript_segments BEGIN
  INSERT INTO transcript_fts(rowid, text) VALUES (new.rowid, new.text); END;
CREATE TRIGGER transcript_fts_ad AFTER DELETE ON transcript_segments BEGIN
  INSERT INTO transcript_fts(transcript_fts, rowid, text) VALUES ('delete', old.rowid, old.text); END;
CREATE TRIGGER transcript_fts_au AFTER UPDATE OF text ON transcript_segments BEGIN
  INSERT INTO transcript_fts(transcript_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
  INSERT INTO transcript_fts(rowid, text) VALUES (new.rowid, new.text); END;

-- Provenance of approved items: the quote and the segment each came from.
CREATE TABLE action_item_meta (
  action_item_id TEXT PRIMARY KEY REFERENCES meeting_action_items(id) ON DELETE CASCADE,
  source_segment_id TEXT REFERENCES transcript_segments(id) ON DELETE SET NULL,
  quote TEXT NOT NULL CHECK (length(quote) <= 1000),
  due_phrase TEXT,
  due_date TEXT CHECK (due_date IS NULL OR due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  priority TEXT CHECK (priority IS NULL OR priority IN ('LOW','MEDIUM','HIGH')),
  confidence REAL
) STRICT;

CREATE TABLE decision_meta (
  decision_id TEXT PRIMARY KEY REFERENCES meeting_decisions(id) ON DELETE CASCADE,
  source_segment_id TEXT REFERENCES transcript_segments(id) ON DELETE SET NULL,
  quote TEXT NOT NULL CHECK (length(quote) <= 1000)
) STRICT;

-- Open questions and risks (the base has no table for them).
CREATE TABLE meeting_insights (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('question','risk')),
  body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 1000),
  source_segment_id TEXT REFERENCES transcript_segments(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_insights_meeting ON meeting_insights (meeting_id);

-- Team glossary → Prisma keyterms (P1, T-505).
CREATE TABLE glossary_terms (
  id TEXT PRIMARY KEY,
  term TEXT NOT NULL UNIQUE CHECK (length(term) BETWEEN 1 AND 100),
  created_at INTEGER NOT NULL
) STRICT;
