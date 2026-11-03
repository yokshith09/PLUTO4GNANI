-- 008: the student pack (T16, owner 2026-10-28): courses, exams, and which course a task or note
-- belongs to. The link lives in its own table per item so tasks and notes stay as they are; deleting
-- a course removes only the links, never a task or a note. The runner owns the transaction and
-- user_version.

CREATE TABLE courses (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 60),
    name_folded TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE exams (
    id         TEXT PRIMARY KEY,
    course_id  TEXT REFERENCES courses(id) ON DELETE CASCADE,
    title      TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    on_date    TEXT NOT NULL CHECK (on_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_exams_date ON exams (on_date);

CREATE TABLE task_course (
    task_id   TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE
) STRICT;
CREATE INDEX idx_task_course_course ON task_course (course_id);

CREATE TABLE note_course (
    note_id   TEXT PRIMARY KEY REFERENCES notes(id) ON DELETE CASCADE,
    course_id TEXT NOT NULL REFERENCES courses(id) ON DELETE CASCADE
) STRICT;
CREATE INDEX idx_note_course_course ON note_course (course_id);
