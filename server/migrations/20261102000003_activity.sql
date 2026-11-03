-- Audit trail: one append-only row per change ("X did Y to Z at time"),
-- written in the same transaction as the change itself. Rows also carry the
-- old and new values, which is what makes "revert this change" possible.
CREATE TABLE activity (
    id              bigserial PRIMARY KEY,
    workspace_id    uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- Deliberately not foreign keys: history outlives what it describes.
    project_id      uuid,
    work_item_id    uuid,
    -- The other item of a relation, so it shows in both items' history.
    related_item_id uuid,
    actor_id        uuid REFERENCES users (id) ON DELETE SET NULL,
    -- Snapshots, so entries still read well after renames and deletions.
    actor_name      text NOT NULL,
    actor_kind      text NOT NULL,
    action          text NOT NULL,
    entity_type     text NOT NULL,
    entity_id       uuid NOT NULL,
    entity_label    text NOT NULL,
    -- {field: {"from": …, "to": …, "from_label"?: …, "to_label"?: …}}
    changes         jsonb NOT NULL DEFAULT '{}',
    reverts         bigint REFERENCES activity (id) ON DELETE SET NULL,
    reverted_by     bigint REFERENCES activity (id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX activity_workspace_idx ON activity (workspace_id, id DESC);
CREATE INDEX activity_project_idx ON activity (project_id, id DESC) WHERE project_id IS NOT NULL;
CREATE INDEX activity_item_idx ON activity (work_item_id, id DESC) WHERE work_item_id IS NOT NULL;
CREATE INDEX activity_related_idx ON activity (related_item_id, id DESC) WHERE related_item_id IS NOT NULL;

-- Soft delete for work items and comments, so deletes can be undone.
--
-- The tables become *_all, and views with the old names show only live
-- rows. Simple views are automatically updatable in Postgres (INSERT,
-- UPDATE, DELETE, RETURNING and ON CONFLICT all pass through), so every
-- existing query keeps working and ignores deleted rows without changes.
-- Note: `SELECT *` fixes the view's columns when it's created; a migration
-- that adds a column to a *_all table must recreate its view.
ALTER TABLE work_items RENAME TO work_items_all;
ALTER TABLE work_items_all
    ADD COLUMN deleted_at timestamptz,
    ADD COLUMN deleted_by uuid REFERENCES users (id) ON DELETE SET NULL;
CREATE VIEW work_items AS SELECT * FROM work_items_all WHERE deleted_at IS NULL;

ALTER TABLE comments RENAME TO comments_all;
ALTER TABLE comments_all
    ADD COLUMN deleted_at timestamptz,
    ADD COLUMN deleted_by uuid REFERENCES users (id) ON DELETE SET NULL;
CREATE VIEW comments AS SELECT * FROM comments_all WHERE deleted_at IS NULL;
