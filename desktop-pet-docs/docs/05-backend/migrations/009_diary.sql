-- 009: the private diary (FR-86, T16). Only ciphertext is stored: the key is derived from the
-- password (Argon2id) and never written. Binary values are stored as lower-case hex text, so the
-- generic export and import keep working and carry the diary encrypted. The runner owns the
-- transaction and user_version.

CREATE TABLE diary_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    salt        TEXT NOT NULL,
    check_nonce TEXT NOT NULL,
    check_data  TEXT NOT NULL,   -- a known phrase encrypted with the key: wrong password = cannot open
    created_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE diary_entries (
    day        TEXT PRIMARY KEY CHECK (day GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    nonce      TEXT NOT NULL,
    data       TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
