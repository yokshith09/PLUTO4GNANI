-- 002: the Bin (soft delete, 30-day retention) and time-triggered reminders.
-- Added at the owner's request (v1.3). The runner owns the transaction and user_version.

-- A note in the Bin keeps its row and its note_labels links; deleted_at is ms since the epoch (UTC).
ALTER TABLE notes ADD COLUMN deleted_at INTEGER;

-- Bin view and the retention sweep only ever look at binned rows.
CREATE INDEX idx_notes_bin ON notes (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE TABLE reminders (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    remind_at   INTEGER NOT NULL,                       -- ms since the epoch (UTC)
    note_id     TEXT REFERENCES notes(id) ON DELETE SET NULL,
    fired_at    INTEGER,                                -- set once ReminderDue has been published
    done_at     INTEGER,
    created_at  INTEGER NOT NULL
) STRICT;

-- The scheduler asks "what is the next pending reminder?" and "what is due?".
CREATE INDEX idx_reminders_pending ON reminders (remind_at) WHERE done_at IS NULL AND fired_at IS NULL;
CREATE INDEX idx_reminders_note ON reminders (note_id) WHERE note_id IS NOT NULL;
