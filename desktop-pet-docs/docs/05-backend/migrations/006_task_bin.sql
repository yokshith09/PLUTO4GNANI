-- 006: a Bin for tasks (FR-46 auto-delete, owner 2026-10-23). Old completed tasks move here and
-- can be restored; after 30 days in the Bin they are deleted for good.
-- The runner owns the transaction and user_version.

ALTER TABLE tasks ADD COLUMN binned_at INTEGER;
CREATE INDEX idx_tasks_binned ON tasks (binned_at) WHERE binned_at IS NOT NULL;
