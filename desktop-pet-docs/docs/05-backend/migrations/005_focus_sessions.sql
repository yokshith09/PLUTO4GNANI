-- 005: focus sessions (FR-05, owner 2026-10-23). A focus or break countdown, saved so that
-- focus time counts as study hours later and a session survives a restart.
-- The runner owns the transaction and user_version.

CREATE TABLE focus_sessions (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('FOCUS', 'BREAK')),
    task_id     TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    -- The task's title when it started, kept if the task is deleted later.
    label       TEXT NOT NULL DEFAULT '' CHECK (length(label) <= 300),
    planned_ms  INTEGER NOT NULL CHECK (planned_ms BETWEEN 60000 AND 14400000),
    started_at  INTEGER NOT NULL,
    paused_at   INTEGER,                                   -- set while paused
    paused_ms   INTEGER NOT NULL DEFAULT 0 CHECK (paused_ms >= 0),
    ended_at    INTEGER,
    outcome     TEXT CHECK (outcome IS NULL OR outcome IN ('DONE', 'STOPPED')),
    local_date  TEXT NOT NULL CHECK (local_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    CHECK ((ended_at IS NULL) = (outcome IS NULL)),
    CHECK (ended_at IS NULL OR paused_at IS NULL)
) STRICT;

-- Only one session runs at a time.
CREATE UNIQUE INDEX idx_focus_one_running ON focus_sessions ((1)) WHERE ended_at IS NULL;
CREATE INDEX idx_focus_day ON focus_sessions (local_date);
