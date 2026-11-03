# Data Schema — Pluto Voice

**Authority** for the tables added by this project. The base's tables (migrations 001–011) are not repeated: they are in `desktop-pet-docs/docs/05-backend/migrations/`.

**Conventions (from the base, ADR-007):**
- Forward-only SQL files, registered in `crates/pluto-core/src/db/migrate.rs`; the runner owns the transaction and `PRAGMA user_version`, so files contain no `BEGIN`/`COMMIT`.
- `STRICT` tables, `TEXT` ids (UUID v7 from `pluto_core::ids::new_id()`), integer milliseconds (UTC) for timestamps, `TEXT 'YYYY-MM-DD'` for dates.
- **Enums are stored as the wire spells them.** Existing enums are `SCREAMING_SNAKE_CASE` (`'IN_PROGRESS'`); the new enums below are `snake_case` (`'action_item'`). No conversion layer is needed.

## 1. Existing tables this project uses (unchanged)
| Table | Used for |
|---|---|
| `meetings` (`id`, `title`, `starts_at`, `notes`, `transcript`, `follow_up_date`, …) | The meeting row. The existing `transcript` column holds the plain joined transcript |
| `meeting_participants` (`meeting_id`, `position`, `name`) | Participants; `position` is the key used by speaker mapping |
| `meeting_decisions` (`id`, `meeting_id`, `position`, `body` ≤ 2000) | Approved decisions |
| `meeting_action_items` (`id`, `meeting_id`, `position`, `body` ≤ 300, `assignee`, `task_id`) | Approved action items; `task_id` links to the created task |
| `tasks` (`status`: `PLANNED / IN_PROGRESS / PENDING / COMPLETED / CANCELLED`, `due_date`, `project`, `source_action_item_id`, …) | Tasks, including those created from action items |
| `reminders`, `notes`, `focus_sessions`, settings and prefs | Voice Assistant reads and writes these through core functions |

## 2. Migration `012_meeting_ai.sql` — owner **D2**
```sql
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
```

## 3. Migration `013_voice.sql` — owner **D1**
```sql
-- Assistant interactions: conversation memory and latency metrics.
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
  outcome TEXT CHECK (outcome IS NULL OR outcome IN
    ('answered','executed','rejected','expired','failed','cancelled')),
  stt_latency_ms INTEGER,
  llm_latency_ms INTEGER,
  tts_first_audio_ms INTEGER
) STRICT;

-- Proposals: the write gate.
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

-- Outbound data log.
CREATE TABLE privacy_log (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  destination TEXT NOT NULL CHECK (destination IN
    ('gnani_realtime','gnani_rest','gnani_batch','gnani_tts','gnani_clone','gemini','groq')),
  purpose TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  ref_id TEXT
) STRICT;
CREATE INDEX idx_privacy_at ON privacy_log (at DESC);
```

## 4. Settings keys (Pluto's settings service)
`voice.cloud_enabled`, `voice.languages`, `voice.stt_format`, `voice.tts_voice`, `voice.tts_model`, `voice.audio_tags`, `voice.always_on`, `llm.gemini_model`, `llm.groq_model`, and the shortcut `shortcuts.voice_ptt` (added with the other `shortcuts.*` keys). API keys are **never** stored in the database.

## 5. Relationships
```
meetings 1─1 meeting_ai ; 1─* participants, decisions, action_items (existing), recordings, jobs, segments (─ transcript_fts), insights
meeting_action_items 1─1 action_item_meta ─> transcript_segments ; 0..1─1 tasks (existing task_id / source_action_item_id)
meeting_decisions    1─1 decision_meta    ─> transcript_segments
voice_interactions 1─* voice_proposals
```

## 6. Ownership and rules
| Data | Written by | Read by |
|---|---|---|
| `meeting_ai`, recordings, jobs, segments | D1 (`batch.rs`, poller) writes jobs and segments; D2 writes recordings and status | D2 (UI, extraction, recall) |
| `action_item_meta`, `decision_meta`, `meeting_insights` | D2 (`meeting_extraction_apply`) | D2 |
| `voice_interactions`, `voice_proposals`, `privacy_log` | D1 | D2 (UI) |
| Existing tables | Through `pluto-core` functions only; never raw SQL from new crates | — |

- Deleting a meeting cascades to its new rows; the service also deletes its recording files.
- Deleting a task leaves the action item with `task_id = NULL`.
- Data export/import (`data_export`/`data_import`) must be extended to include the new tables (task T-106 for 012, T-107 for 013). Recording audio files are excluded from export by default.

## 7. Migration tests
| ID | Test |
|---|---|
| T-DB-01 | The full chain 001–013 applies on an empty database; `user_version` = 13 |
| T-DB-02 | A mic recording without consent is rejected |
| T-DB-03 | An FTS5 `MATCH` returns inserted segments (this also proves the bundled SQLite has FTS5; if not, fall back to `LIKE`) |
| T-DB-04 | Deleting a meeting cascades to its new rows |
| T-DB-05 | The constraints on interactions and proposals hold |
