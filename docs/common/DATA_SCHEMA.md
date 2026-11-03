# Data Schema — Pluto Voice

**Authority:** this file defines all tables. Types in `API_CONTRACTS.md` map onto them.

**Storage:**
- One local SQLite database in the app data folder.
- `rusqlite` (bundled) with FTS5.
- Forward-only migrations in `crates/core/migrations/NNN_name.sql`, applied by a runner that records `PRAGMA user_version`.

**Ownership:** Developer 1 owns the migration files. Developer 2 reviews them.

## Migration `001_init.sql`
```sql
PRAGMA foreign_keys = ON;

-- ───────── Personal productivity ─────────
CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
  color TEXT,
  archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE tasks (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
  notes TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'TODO' CHECK (status IN ('TODO','IN_PROGRESS','DONE','CANCELLED')),
  priority TEXT CHECK (priority IN ('LOW','MEDIUM','HIGH')),
  due_date TEXT CHECK (due_date IS NULL OR due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  source_action_item_id TEXT,                 -- set when created from a meeting action item
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  completed_at INTEGER,
  CHECK ((status = 'DONE') = (completed_at IS NOT NULL))
) STRICT;
CREATE INDEX idx_tasks_status_due ON tasks (status, due_date);

CREATE TABLE reminders (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  remind_at INTEGER NOT NULL,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  fired_at INTEGER,
  done_at INTEGER,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_reminders_pending ON reminders (remind_at) WHERE done_at IS NULL;

CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL DEFAULT '' CHECK (length(title) <= 200),
  body TEXT NOT NULL DEFAULT '' CHECK (length(body) <= 100000),
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE VIRTUAL TABLE notes_fts USING fts5(title, body, content='notes', content_rowid='rowid');
-- triggers keep notes_fts in sync (insert / delete / update), same pattern as transcript_fts below

CREATE TABLE focus_sessions (
  id TEXT PRIMARY KEY,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  planned_minutes INTEGER NOT NULL CHECK (planned_minutes BETWEEN 1 AND 240),
  started_at INTEGER NOT NULL,
  paused_ms INTEGER NOT NULL DEFAULT 0,
  paused_at INTEGER,
  ended_at INTEGER,
  state TEXT NOT NULL CHECK (state IN ('RUNNING','PAUSED','COMPLETED','STOPPED'))
) STRICT;

-- ───────── Meetings ─────────
CREATE TABLE meetings (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
  starts_at INTEGER NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('RECORDING','TRANSCRIBING','NEEDS_SPEAKERS','NEEDS_REVIEW','DONE','FAILED')),
  dominant_language TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE meeting_participants (
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
  PRIMARY KEY (meeting_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE meeting_recordings (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  source TEXT NOT NULL CHECK (source IN ('mic','import','loopback')),
  chunk_index INTEGER NOT NULL,
  offset_ms INTEGER NOT NULL,
  file_path TEXT NOT NULL,
  duration_ms INTEGER,
  consent_confirmed_at INTEGER,
  created_at INTEGER NOT NULL,
  CHECK (source = 'import' OR consent_confirmed_at IS NOT NULL)
) STRICT;

CREATE TABLE transcription_jobs (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  gnani_job_id TEXT,
  status TEXT NOT NULL CHECK (status IN ('QUEUED','UPLOADING','CREATED','IN_PROGRESS','COMPLETED','PARTIAL_FAILURE','FAILED','CANCELLED')),
  config_json TEXT NOT NULL,
  error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE transcript_segments (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  seg_index INTEGER NOT NULL,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  speaker_label INTEGER,
  participant_position INTEGER,
  text TEXT NOT NULL,
  language TEXT,
  relevance TEXT CHECK (relevance IS NULL OR relevance IN ('decision','action_item','question','risk','chitchat')),
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

-- Approved meeting items (decisions, action items, questions, risks) with provenance.
CREATE TABLE meeting_items (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('decision','action_item','question','risk')),
  body TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 1000),
  owner TEXT,
  due_phrase TEXT,
  due_date TEXT CHECK (due_date IS NULL OR due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  priority TEXT CHECK (priority IS NULL OR priority IN ('LOW','MEDIUM','HIGH')),
  source_segment_id TEXT REFERENCES transcript_segments(id) ON DELETE SET NULL,
  quote TEXT NOT NULL CHECK (length(quote) <= 1000),
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_items_meeting ON meeting_items (meeting_id, kind);
CREATE UNIQUE INDEX idx_items_task ON meeting_items (task_id) WHERE task_id IS NOT NULL;

-- ───────── Voice ─────────
CREATE TABLE voice_interactions (
  id TEXT PRIMARY KEY,
  started_at INTEGER NOT NULL,
  transcript TEXT NOT NULL,
  language TEXT,
  intent TEXT,
  decision_source TEXT CHECK (decision_source IS NULL OR decision_source IN ('rules','llm')),
  llm_provider TEXT CHECK (llm_provider IS NULL OR llm_provider IN ('gemini','groq')),
  tool TEXT,
  args_json TEXT,
  outcome TEXT CHECK (outcome IS NULL OR outcome IN ('answered','executed','rejected','expired','failed','cancelled')),
  stt_latency_ms INTEGER,
  llm_latency_ms INTEGER,
  tts_first_audio_ms INTEGER
) STRICT;

CREATE TABLE voice_proposals (
  id TEXT PRIMARY KEY,
  interaction_id TEXT NOT NULL REFERENCES voice_interactions(id) ON DELETE CASCADE,
  tool TEXT NOT NULL,
  args_json TEXT NOT NULL,
  summary_text TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending','approved','rejected','expired','failed')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  decided_at INTEGER
) STRICT;

CREATE TABLE privacy_log (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  destination TEXT NOT NULL CHECK (destination IN
    ('gnani_realtime','gnani_rest','gnani_batch','gnani_tts','gnani_clone','gemini','groq')),
  purpose TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  ref_id TEXT
) STRICT;

CREATE TABLE glossary_terms (
  id TEXT PRIMARY KEY,
  term TEXT NOT NULL UNIQUE CHECK (length(term) BETWEEN 1 AND 100),
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;
```

## Relationships
```
projects 1─* tasks, notes
tasks 1─* reminders, focus_sessions
meetings 1─* participants, recordings, transcription_jobs, transcript_segments (─ transcript_fts), meeting_items
meeting_items(action_item) 0..1─1 tasks (task_id; tasks.source_action_item_id points back)
voice_interactions 1─* voice_proposals
```

## Settings keys
`voice.cloud_enabled`, `voice.languages`, `voice.stt_format`, `voice.tts_voice`, `voice.tts_model`, `voice.hotkey`, `voice.audio_tags`, `voice.always_on`, `llm.gemini_model`, `llm.groq_model`.

API keys are **never** stored in SQLite.

## Data rules
| Data | Written by | Read by |
|---|---|---|
| Personal tables | D1 (core services) | D2 (UI), assistant tools |
| Recordings, jobs, segments | D1 | D2 (transcript UI, extraction, recall) |
| `meeting_items` | D2 (`meeting_extraction_apply`) | D2, assistant tools |
| Interactions, proposals, privacy log | D1 | D2 (UI) |

- Deleting a meeting cascades to its rows and deletes its recording files.
- Deleting a task leaves the meeting item with `task_id = NULL`. The commitment then shows `NoTask`.

## Migration tests
| ID | Test |
|---|---|
| T-DB-01 | Migrations apply on an empty database; `user_version` = 1 |
| T-DB-02 | A mic recording without consent is rejected |
| T-DB-03 | FTS5 MATCH returns inserted segments and notes. **If FTS5 is missing:** fall back to `LIKE` |
| T-DB-04 | Meeting delete cascades |
| T-DB-05 | The task `DONE` ↔ `completed_at` check holds |
