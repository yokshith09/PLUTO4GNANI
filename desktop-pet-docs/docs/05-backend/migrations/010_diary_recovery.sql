-- 010: the diary gets a recovery key (owner 2026-10-28: a forgotten password must not lose the
-- diary). Entries are now encrypted with a random data key that is stored twice: wrapped by the
-- password-derived key and by the recovery key. The diary from 009 was never released, and its
-- entries were encrypted straight from the password, so it is rebuilt empty. The runner owns the
-- transaction and user_version.

DROP TABLE diary_entries;
DROP TABLE diary_meta;

CREATE TABLE diary_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    salt        TEXT NOT NULL,
    wrap_nonce  TEXT NOT NULL,
    wrapped     TEXT NOT NULL,   -- the data key, wrapped by the password
    rec_salt    TEXT NOT NULL,
    rec_nonce   TEXT NOT NULL,
    rec_wrapped TEXT NOT NULL,   -- the same data key, wrapped by the recovery key
    created_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE diary_entries (
    day        TEXT PRIMARY KEY CHECK (day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    nonce      TEXT NOT NULL,
    data       TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
