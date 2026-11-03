-- 011: crash-safe time tracking. The stretch in front right now is written here every look
-- (about a minute), so a crash, a kill or a power cut loses at most that minute instead of the
-- whole stretch. At the next start the row is turned into a normal session ending at `last_seen`
-- and removed. The runner owns the transaction and user_version.
--
-- Same privacy rules as 003: an app's display name or a bare host, nothing else. At most one row
-- per kind, because only one app and one site can be in front at once. Transient: it is not
-- exported, and every "delete" of usage data clears it.
CREATE TABLE usage_open (
    kind        TEXT PRIMARY KEY CHECK (kind IN ('app', 'site')),
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 253),  -- app display name, or bare host
    category    TEXT NOT NULL DEFAULT 'Other',                          -- apps only
    is_browser  INTEGER NOT NULL DEFAULT 0 CHECK (is_browser IN (0, 1)),
    browser     TEXT NOT NULL DEFAULT '',                               -- sites only
    started_at  INTEGER NOT NULL,
    last_seen   INTEGER NOT NULL CHECK (last_seen >= started_at)
) STRICT;
