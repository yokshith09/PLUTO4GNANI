-- 007: projects for notes, and reminders that belong to a task (owner 2026-10-25).
-- A note can sit in a project, like a task already can (tasks.project). A reminder can belong
-- to a task, remembers how many minutes before the moment it was set for, and whether it plays
-- a sound. The runner owns the transaction and user_version.

ALTER TABLE notes ADD COLUMN project TEXT CHECK (project IS NULL OR length(project) BETWEEN 1 AND 100);
CREATE INDEX idx_notes_project ON notes (project) WHERE project IS NOT NULL;

ALTER TABLE reminders ADD COLUMN task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL;
ALTER TABLE reminders ADD COLUMN lead_min INTEGER NOT NULL DEFAULT 0 CHECK (lead_min BETWEEN 0 AND 1440);
ALTER TABLE reminders ADD COLUMN sound INTEGER NOT NULL DEFAULT 1 CHECK (sound IN (0, 1));
CREATE INDEX idx_reminders_task ON reminders (task_id) WHERE task_id IS NOT NULL;
