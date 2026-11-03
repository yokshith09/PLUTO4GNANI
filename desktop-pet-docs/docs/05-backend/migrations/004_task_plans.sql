-- 004: task upgrades (owner, 2026-10-23): links on a task (FR-106), stars (FR-107),
-- chosen days (FR-108) and repeating tasks ticked per day (FR-109).
-- The runner owns the transaction and user_version.

-- A star puts a task or reminder first. 0 or 1.
ALTER TABLE tasks ADD COLUMN starred INTEGER NOT NULL DEFAULT 0 CHECK (starred IN (0, 1));
ALTER TABLE reminders ADD COLUMN starred INTEGER NOT NULL DEFAULT 0 CHECK (starred IN (0, 1));

-- How a task repeats: 'daily', 'weekly:1,3,5' (ISO weekdays, Monday = 1) or 'every:N' days,
-- counted from planned_date. NULL = it doesn't repeat. repeat_until is the last possible day.
ALTER TABLE tasks ADD COLUMN repeat_rule TEXT
    CHECK (repeat_rule IS NULL OR repeat_rule = 'daily' OR repeat_rule GLOB 'weekly:[1-7]*' OR repeat_rule GLOB 'every:[1-9]*');
ALTER TABLE tasks ADD COLUMN repeat_until TEXT
    CHECK (repeat_until IS NULL OR repeat_until GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');

-- Links attached to a task, shown next to it. Only http(s)/mailto are stored (checked in code).
CREATE TABLE task_links (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    url         TEXT NOT NULL CHECK (length(url) BETWEEN 1 AND 2048),
    title       TEXT NOT NULL DEFAULT '' CHECK (length(title) <= 200),
    position    INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_task_links_task ON task_links (task_id, position);

-- The specific days a task is planned on, when there is more than one.
CREATE TABLE task_days (
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    day         TEXT NOT NULL CHECK (day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    PRIMARY KEY (task_id, day)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_task_days_day ON task_days (day);

-- One row per day a repeating task was ticked. A day without a row was not ticked.
CREATE TABLE task_ticks (
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    day         TEXT NOT NULL CHECK (day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    done_at     INTEGER NOT NULL,
    PRIMARY KEY (task_id, day)
) STRICT, WITHOUT ROWID;
