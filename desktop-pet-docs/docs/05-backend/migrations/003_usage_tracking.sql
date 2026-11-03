-- 003: "where my time went" (Phase 2 pulled forward by the owner, v1.4). Opt-in, local only.
-- The runner owns the transaction and user_version.
--
-- Privacy rules, enforced by the shape of these tables:
--   * Domain only. There is no column for a URL, a path, a window title or a tab title, and
--     none may be added without a new ADR.
--   * Incognito/private windows are never reported (the browser extension's job, PRD 4.7).
--   * All times are ms since the epoch (UTC).

-- One row per continuous stretch of one foreground application.
CREATE TABLE app_sessions (
    id          TEXT PRIMARY KEY,
    app         TEXT NOT NULL CHECK (length(app) BETWEEN 1 AND 100),   -- display name, e.g. 'Google Chrome'
    category    TEXT NOT NULL,                                         -- Coding, Design, Research, ...
    is_browser  INTEGER NOT NULL CHECK (is_browser IN (0, 1)),
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER NOT NULL CHECK (ended_at >= started_at)
) STRICT;

CREATE INDEX idx_app_sessions_started ON app_sessions (started_at);

-- One row per continuous stretch on one site, reported by a browser extension.
CREATE TABLE domain_sessions (
    id          TEXT PRIMARY KEY,
    browser     TEXT NOT NULL CHECK (length(browser) BETWEEN 1 AND 100),
    domain      TEXT NOT NULL CHECK (length(domain) BETWEEN 1 AND 253),  -- bare lower-case host
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER NOT NULL CHECK (ended_at >= started_at)
) STRICT;

CREATE INDEX idx_domain_sessions_domain ON domain_sessions (domain, started_at);

-- The user's say over each site: its category, and whether it is tracked at all.
-- A domain with tracked = 0 is never written to domain_sessions.
CREATE TABLE domains (
    domain      TEXT PRIMARY KEY,
    category    TEXT NOT NULL DEFAULT 'Other',
    tracked     INTEGER NOT NULL DEFAULT 1 CHECK (tracked IN (0, 1)),
    first_seen  INTEGER NOT NULL,
    last_seen   INTEGER NOT NULL
) STRICT;
