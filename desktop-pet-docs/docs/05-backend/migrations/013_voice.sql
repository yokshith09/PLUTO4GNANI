-- 013: voice — assistant interactions, proposals (the write gate), privacy log
-- (DATA_SCHEMA.md §3). Owner: D1 (T-107). The runner owns the transaction and user_version.
--
-- Enums are stored in their wire spelling (snake_case). All three tables are exported and
-- imported like any other table, and cleared by "delete all".

-- Assistant interactions: conversation memory and latency metrics.
CREATE TABLE voice_interactions (
  id TEXT PRIMARY KEY,
  started_at INTEGER NOT NULL,
  transcript TEXT NOT NULL,
  language TEXT,
  intent TEXT,
  decision_source TEXT CHECK (decision_source IS NULL OR decision_source IN ('rules','llm')),
  llm_provider TEXT CHECK (llm_provider IS NULL OR llm_provider IN ('gemini','groq')),
  tool TEXT,
  args_json TEXT,
  outcome TEXT CHECK (outcome IS NULL OR outcome IN
    ('answered','executed','rejected','expired','failed','cancelled')),
  stt_latency_ms INTEGER,
  llm_latency_ms INTEGER,
  tts_first_audio_ms INTEGER
) STRICT;

-- Proposals: the write gate.
CREATE TABLE voice_proposals (
  id TEXT PRIMARY KEY,
  interaction_id TEXT NOT NULL REFERENCES voice_interactions(id) ON DELETE CASCADE,
  tool TEXT NOT NULL,
  args_json TEXT NOT NULL,
  summary_text TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending','approved','rejected','expired','failed')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  decided_at INTEGER
) STRICT;

-- Outbound data log.
CREATE TABLE privacy_log (
  id INTEGER PRIMARY KEY,
  at INTEGER NOT NULL,
  destination TEXT NOT NULL CHECK (destination IN
    ('gnani_realtime','gnani_rest','gnani_batch','gnani_tts','gnani_clone','gemini','groq')),
  purpose TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  ref_id TEXT
) STRICT;
CREATE INDEX idx_privacy_at ON privacy_log (at DESC);
