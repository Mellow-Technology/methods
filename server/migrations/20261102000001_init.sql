-- Methods: initial schema.
-- Enumerations are stored as text with CHECK constraints so they are easy to
-- extend and simple to map from Rust.

CREATE TABLE files (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    object_key   text NOT NULL UNIQUE,
    file_name    text NOT NULL,
    content_type text NOT NULL,
    size_bytes   bigint NOT NULL,
    uploaded_by  uuid,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    kind             text NOT NULL CHECK (kind IN ('human', 'bot')),
    display_name     text NOT NULL,
    first_name       text,
    last_name        text,
    email            text,
    password_hash    text,
    avatar_file_id   uuid REFERENCES files (id) ON DELETE SET NULL,
    cover_file_id    uuid REFERENCES files (id) ON DELETE SET NULL,
    oidc_issuer      text,
    oidc_subject     text,
    -- Bots belong to exactly one workspace.
    bot_workspace_id uuid,
    created_by       uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CHECK (kind = 'bot' OR email IS NOT NULL),
    CHECK (kind = 'human' OR bot_workspace_id IS NOT NULL)
);

CREATE UNIQUE INDEX users_email_key ON users (lower(email)) WHERE email IS NOT NULL;
CREATE UNIQUE INDEX users_oidc_key ON users (oidc_issuer, oidc_subject) WHERE oidc_subject IS NOT NULL;

ALTER TABLE files
    ADD CONSTRAINT files_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES users (id) ON DELETE SET NULL;

-- Browser sessions for human users. Only a hash of the token is stored.
CREATE TABLE sessions (
    token_hash text PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

-- Access tokens bots use to connect over MCP (and the REST API).
CREATE TABLE bot_tokens (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    bot_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         text NOT NULL,
    token_prefix text NOT NULL,
    token_hash   text NOT NULL UNIQUE,
    created_by   uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at   timestamptz
);
CREATE INDEX bot_tokens_bot_idx ON bot_tokens (bot_id);

-- In-flight OIDC logins (state -> nonce + PKCE verifier).
CREATE TABLE oidc_states (
    state         text PRIMARY KEY,
    nonce         text NOT NULL,
    pkce_verifier text NOT NULL,
    return_to     text NOT NULL DEFAULT '/',
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE workspaces (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name       text NOT NULL,
    slug       text NOT NULL UNIQUE,
    created_by uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE users
    ADD CONSTRAINT users_bot_workspace_fkey FOREIGN KEY (bot_workspace_id) REFERENCES workspaces (id) ON DELETE CASCADE;

CREATE TABLE workspace_members (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role         text NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

CREATE TABLE projects (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name          text NOT NULL,
    -- The human-facing "Project ID", used as the work item key prefix (MTH-12).
    identifier    text NOT NULL CHECK (identifier ~ '^[A-Z][A-Z0-9]{1,9}$'),
    description   text NOT NULL DEFAULT '',
    cover_file_id uuid REFERENCES files (id) ON DELETE SET NULL,
    icon_file_id  uuid REFERENCES files (id) ON DELETE SET NULL,
    icon_emoji    text,
    next_seq      integer NOT NULL DEFAULT 1,
    created_by    uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, identifier)
);

CREATE TABLE epics (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  uuid NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name        text NOT NULL,
    description text NOT NULL DEFAULT '',
    color       text NOT NULL DEFAULT '#6366f1',
    created_by  uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX epics_project_idx ON epics (project_id);

CREATE TABLE labels (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id uuid NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       text NOT NULL,
    color      text NOT NULL DEFAULT '#64748b',
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX labels_project_name_key ON labels (project_id, lower(name));

CREATE TABLE work_items (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  uuid NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    seq         integer NOT NULL,
    title       text NOT NULL,
    description text NOT NULL DEFAULT '',
    state       text NOT NULL DEFAULT 'backlog'
                CHECK (state IN ('backlog', 'todo', 'in_progress', 'done', 'cancelled')),
    priority    text NOT NULL DEFAULT 'medium'
                CHECK (priority IN ('urgent', 'high', 'medium', 'low')),
    assignee_id uuid REFERENCES users (id) ON DELETE SET NULL,
    created_by  uuid REFERENCES users (id) ON DELETE SET NULL,
    start_date  date,
    due_date    date,
    parent_id   uuid REFERENCES work_items (id) ON DELETE SET NULL,
    epic_id     uuid REFERENCES epics (id) ON DELETE SET NULL,
    -- Position within its kanban column.
    sort_order  double precision NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (project_id, seq),
    CHECK (parent_id IS NULL OR parent_id <> id),
    CHECK (due_date IS NULL OR start_date IS NULL OR due_date >= start_date)
);
CREATE INDEX work_items_board_idx ON work_items (project_id, state, sort_order);
CREATE INDEX work_items_parent_idx ON work_items (parent_id);
CREATE INDEX work_items_epic_idx ON work_items (epic_id);
CREATE INDEX work_items_assignee_idx ON work_items (assignee_id);

CREATE TABLE work_item_labels (
    work_item_id uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    label_id     uuid NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (work_item_id, label_id)
);

-- Relations are normalised on write: "blocked by" is stored as the inverse
-- "blocks", and "relates to" with the lower id first, so each fact has one row.
CREATE TABLE work_item_relations (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id  uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    target_id  uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    kind       text NOT NULL CHECK (kind IN ('relates_to', 'duplicate_of', 'blocked_by', 'blocks')),
    created_by uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source_id, target_id, kind),
    CHECK (source_id <> target_id)
);
CREATE INDEX work_item_relations_target_idx ON work_item_relations (target_id);

CREATE TABLE work_item_links (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    url          text NOT NULL,
    title        text,
    created_by   uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX work_item_links_item_idx ON work_item_links (work_item_id);

CREATE TABLE work_item_attachments (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    file_id      uuid NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    created_by   uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX work_item_attachments_item_idx ON work_item_attachments (work_item_id);

CREATE TABLE comments (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    work_item_id uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    author_id    uuid REFERENCES users (id) ON DELETE SET NULL,
    -- Sanitised HTML.
    body_html    text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX comments_item_idx ON comments (work_item_id, created_at);

CREATE TABLE work_item_reactions (
    work_item_id uuid NOT NULL REFERENCES work_items (id) ON DELETE CASCADE,
    user_id      uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    emoji        text NOT NULL CHECK (char_length(emoji) BETWEEN 1 AND 16),
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (work_item_id, user_id, emoji)
);
