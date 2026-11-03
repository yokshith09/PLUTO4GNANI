-- Pluto · Migration 001 · Phase 0 + Phase 1 schema
-- Conventions:
--   ids          TEXT  UUIDv7 (time-sortable), generated in Rust
--   *_at         INTEGER Unix epoch milliseconds, UTC
--   *_date       TEXT  'YYYY-MM-DD' in the user's local calendar
--   booleans     INTEGER 0/1 with CHECK
-- Connection PRAGMAs (set by Rust on every open, not here):
--   journal_mode=WAL, foreign_keys=ON, synchronous=NORMAL, busy_timeout=5000
--
-- TRANSACTION OWNERSHIP (ADR-007, backend-schema.md §5):
--   The migration runner wraps every migration file in a single transaction and
--   sets `PRAGMA user_version` after the file applies cleanly. This file must
--   therefore contain NO `BEGIN`/`COMMIT` and NO `PRAGMA user_version` — a
--   nested BEGIN fails at runtime, and a version set inside the file can drift
--   from the runner's own bookkeeping.

-- ───────────────────────── Settings & preferences ─────────────────────────
-- settings          : user-controlled configuration (exported)
-- user_preferences  : UI state the app remembers (window size, pet position, last view)
CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL CHECK (json_valid(value)),
    updated_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE user_preferences (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL CHECK (json_valid(value)),
    updated_at  INTEGER NOT NULL
) STRICT;

-- ───────────────────────────────── Notes ─────────────────────────────────
CREATE TABLE notes (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL DEFAULT '' CHECK (length(title) <= 200),
    body        TEXT NOT NULL DEFAULT '' CHECK (length(body) <= 100000),
    color       TEXT NOT NULL DEFAULT 'default'
                CHECK (color IN ('default','red','orange','yellow','green','teal','blue','purple','gray')),
    pinned      INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0,1)),
    archived    INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0,1)),
    created_at  INTEGER NOT NULL,
    edited_at   INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_notes_active ON notes (archived, pinned DESC, edited_at DESC);

-- `name`        : the label as the user typed it (display form)
-- `name_folded` : Unicode-lowercased in Rust; the authoritative uniqueness key.
--                 SQLite's NOCASE collation folds ASCII A–Z only, so it cannot
--                 enforce "unique case-insensitively" for non-ASCII names
--                 (É/é, İ/i). Folding in Rust and indexing the folded column
--                 makes the DB the real constraint, not a partial one.
CREATE TABLE labels (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    name_folded TEXT NOT NULL CHECK (length(name_folded) BETWEEN 1 AND 50),
    created_at  INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX idx_labels_name_folded ON labels (name_folded);

CREATE TABLE note_labels (
    note_id     TEXT NOT NULL REFERENCES notes(id)  ON DELETE CASCADE,
    label_id    TEXT NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    PRIMARY KEY (note_id, label_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_note_labels_label ON note_labels (label_id);

-- ───────────────────────────────── Tasks ─────────────────────────────────
CREATE TABLE tasks (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 300),
    description   TEXT NOT NULL DEFAULT '',
    status        TEXT NOT NULL DEFAULT 'PLANNED'
                  CHECK (status IN ('PLANNED','IN_PROGRESS','PENDING','COMPLETED','CANCELLED')),
    priority      TEXT CHECK (priority IN ('LOW','MEDIUM','HIGH')),
    project       TEXT,
    planned_date  TEXT CHECK (planned_date IS NULL OR planned_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    due_date      TEXT CHECK (due_date     IS NULL OR due_date     GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    note_id       TEXT REFERENCES notes(id) ON DELETE SET NULL,
    source_action_item_id TEXT,              -- set when created from a meeting action item
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    started_at    INTEGER,
    completed_at  INTEGER,
    cancelled_at  INTEGER,
    CHECK (status <> 'COMPLETED' OR completed_at IS NOT NULL),
    CHECK (status =  'COMPLETED' OR completed_at IS NULL)
) STRICT;

CREATE INDEX idx_tasks_status_planned ON tasks (status, planned_date);
CREATE INDEX idx_tasks_due            ON tasks (due_date) WHERE due_date IS NOT NULL;
CREATE INDEX idx_tasks_completed      ON tasks (completed_at) WHERE completed_at IS NOT NULL;
CREATE INDEX idx_tasks_project        ON tasks (project) WHERE project IS NOT NULL;

-- Append-only status/date history. Source of truth for daily-log reconstruction.
CREATE TABLE task_events (
    id            INTEGER PRIMARY KEY,       -- rowid, monotonic
    task_id       TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    -- No 'DELETED' kind: task_events.task_id cascades, so a row recording a
    -- task's deletion would be erased by that same deletion (see §3.4).
    kind          TEXT NOT NULL CHECK (kind IN ('CREATED','STATUS','DEFERRED','EDITED')),
    from_status   TEXT,
    to_status     TEXT,
    from_date     TEXT,
    to_date       TEXT,
    at            INTEGER NOT NULL,
    local_date    TEXT NOT NULL              -- local calendar day of `at`, stored to survive TZ changes
) STRICT;

CREATE INDEX idx_task_events_day  ON task_events (local_date);
CREATE INDEX idx_task_events_task ON task_events (task_id, at);

CREATE TRIGGER task_events_no_update BEFORE UPDATE ON task_events
BEGIN SELECT RAISE(ABORT, 'task_events is append-only'); END;

CREATE TABLE task_work_updates (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 5000),
    created_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_work_updates_task ON task_work_updates (task_id, created_at DESC);

-- ──────────────────────────────── Meetings ───────────────────────────────
CREATE TABLE meetings (
    id              TEXT PRIMARY KEY,
    title           TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    starts_at       INTEGER NOT NULL,
    notes           TEXT NOT NULL DEFAULT '',
    transcript      TEXT,                    -- Phase 6 (Voice)
    follow_up_date  TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_meetings_starts ON meetings (starts_at DESC);

CREATE TABLE meeting_participants (
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    PRIMARY KEY (meeting_id, position)
) STRICT, WITHOUT ROWID;

-- NOCASE is fine here: this index serves participant autocomplete (a prefix
-- lookup), not a uniqueness guarantee, so its ASCII-only folding costs nothing
-- beyond a non-ASCII name occasionally sorting as a separate suggestion.
-- Contrast `labels.name_folded`, where uniqueness is the point.
CREATE INDEX idx_participants_name ON meeting_participants (name COLLATE NOCASE);

CREATE TABLE meeting_decisions (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 2000)
) STRICT;

CREATE INDEX idx_decisions_meeting ON meeting_decisions (meeting_id, position);

CREATE TABLE meeting_action_items (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 300),
    assignee    TEXT,
    task_id     TEXT REFERENCES tasks(id) ON DELETE SET NULL
) STRICT;

CREATE INDEX idx_action_items_meeting ON meeting_action_items (meeting_id, position);
CREATE UNIQUE INDEX idx_action_items_task ON meeting_action_items (task_id) WHERE task_id IS NOT NULL;

-- ─────────────────────────────── Daily logs ──────────────────────────────
-- One immutable snapshot per local day. Today's log is computed live, never stored.
CREATE TABLE daily_logs (
    log_date        TEXT PRIMARY KEY,         -- 'YYYY-MM-DD'
    snapshot        TEXT NOT NULL CHECK (json_valid(snapshot)),
    snapshot_version INTEGER NOT NULL,        -- shape version of the JSON
    generated_at    INTEGER NOT NULL,
    reconstructed   INTEGER NOT NULL DEFAULT 0 CHECK (reconstructed IN (0,1))
) STRICT;

CREATE TRIGGER daily_logs_immutable BEFORE UPDATE ON daily_logs
BEGIN SELECT RAISE(ABORT, 'daily_logs are immutable'); END;

-- Deletion is allowed only via "Delete all data", which drops and recreates the DB file.
CREATE TRIGGER daily_logs_no_delete BEFORE DELETE ON daily_logs
BEGIN SELECT RAISE(ABORT, 'daily_logs cannot be deleted'); END;

-- No search tables: global search was withdrawn (ADR-017). Re-adding it is a
-- new migration that creates the index and backfills it from the source tables.

-- `PRAGMA user_version = 1` is set by the migration runner, not here (see header).
