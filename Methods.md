Let's create a project management system:

The application will a single page application. The back-end of the application should be built with Rust/Actix with a PostgreSql database for data storage, and any S3 compatible service for object storage. 

The UI will be built with Next.js using the GUI Mellow UI library. The main view of the application for now will be a list of projects. The main project view will be a kanban board of work items with 
each column being a state.



The data should have the following structure:

- Workspaces contain Projects
- Projects contain Work Items
- Work Items can be grouped in Epics
- Work Items can have rich text comments
- Projects have the following fields
	- Name
	- Project ID
	- Description
	- A cover image
	- An Avatar/Icon
- Work Items have the following fields
	- State
		- State should be one of Backlog, Todo, In Progress, Done, Cancelled
	- Assignee (which can be human or bot)
	- Priority
		- One of Urgent, High, Medium, Low
	- Created by (inferred)
	- Start Date
	- Due Date
	- A Parent Work Item
	- Labels (tags)
	- Relations
		- Relates to, Duplicate of, Blocked by, or Blocks
	- A list of URLs (links)
	- Attached files (design files, etc.)
	- Reactions (emoji)

There will be two user types:

- Human users - For human users the important thing is to be able to organize work and have an overview of project status
	- Humans will have these fields
		- First name
		- Last name
		- Display Name
		- E-mail address
		- Avatar
		- Cover image
- For human users we'll assume that they authenticate via OIDC
	- We'll also have email/password login, for now let's assume this is mostly for testing
- Bot users - Bot users will connect to the system via MCP.
	- Bot should have a name
	- Bots should have access tokens to connect via MCP

---

# Implementation status

_Last updated: 2026-11-02_

How to run it is in [README.md](README.md).

## Architecture

```
Browser ──► Next.js static export (web/out) ─┐
                                             │  one origin, served by Actix
MCP client ──► POST /mcp ───────────────────┤
                                             ▼
                     server/ (Rust · Actix Web 4 · sqlx 0.8)
                       ├── PostgreSQL     (all data; migrations embedded, run on start)
                       └── object_store   (S3-compatible bucket, or local disk in dev)
```

- **server/**: a single binary serving `/api/*` (REST + JSON), `/mcp` (Model Context Protocol), and the exported SPA, with an index.html fallback for client routes.
  - `activity.rs`: the audit trail: recording, feeds, undo and restore.
  - `live.rs`: real-time updates over Server-Sent Events.
  - `richtext.rs`: the server's half of the rich text schema. It validates documents, converts Markdown to documents and back, and converts legacy HTML.
  - `service.rs`: the work item logic shared by REST and MCP, so bots and humans go through the same validation and permission checks.
  - `access.rs`: workspace-membership authorization.
- **web/**: Next.js 16 App Router built as a static export (`output: "export"`) and consumed as a pure SPA. Dynamic views use query strings (`/project/?id=…&item=MTH-12`), so no server rendering is needed. In dev, `next dev` proxies `/api` and `/mcp` to the Rust server, keeping cookies same-origin.
- **UI**: gui-mellow (installed from `github:Mellow-Technology/gui-mellow`), with its tokens, light and dark themes, Dialog, Drawer, Tabs, Field and so on. App styles read only `--gm-*` tokens.

## Done

### Data model (PostgreSQL)
- [x] Workspaces → Projects → Work Items, with workspace membership roles (owner / admin / member)
- [x] Projects: name, **Project ID** (`identifier`, e.g. `MTH`, the prefix for item keys such as `MTH-12`), description, cover image, icon (an uploaded image *or* an emoji, falling back to initials)
- [x] Work items: state (Backlog / Todo / In Progress / Done / Cancelled), assignee (human **or** bot), priority (Urgent / High / Medium / Low), created by (inferred from the session or token), start and due dates, parent work item (cycles rejected), labels, relations, links, attachments, emoji reactions
- [x] Epics group work items (name, description, colour, progress counts)
- [x] Rich text descriptions and comments, stored as TipTap's JSON document tree in `jsonb` (not HTML), with comment edit and delete
- [x] Relations: relates to / duplicate of / blocked by / blocks. Each relation is stored once, normalized (blocked by → inverse blocks), and shown from each side's point of view (A *blocks* B ⇔ B *blocked by* A; *duplicated by* for the inverse of duplicate of)
- [x] Per-project sequential keys (`MTH-1`, `MTH-2`, …); items can be fetched by key or id
- [x] Kanban ordering through a fractional `sort_order` within each state column

### History, undo and real time
- [x] **Audit trail.** Every change in a workspace (items, comments, relations, links, attachments, reactions, projects, labels, epics, members, bots, tokens) writes one `activity` row: who (human or bot), what, to which object, when, with old and new values. It's written in the same transaction as the change, so history and data can't disagree.
- [x] **Feeds:** per work item (an Activity section in the panel), per project (an Activity drawer on the board), and per workspace (API). Entries read as sentences, such as "Claude moved MTH-12 from Todo to In Progress".
- [x] **Undo.** Any work item field change can be undone, as long as nobody has changed that field since; otherwise it's refused with a 409 that says which field moved. Comment edits can be undone by their author.
- [x] **Trash.** Deleting an item or comment is a soft delete, restorable from the activity feed ("Restore").
- [x] **Description history.** Every saved version is kept, and any version can be restored. Rapid edits by the same person within 10 minutes fold into one version.
- [x] **Live updates.** Boards, item panels, the projects list and settings follow changes made by anyone (people in other tabs, bots over MCP) within about 300 ms. A dot in the header shows the connection status.
- [x] MCP `get_activity` tool, so bots can read history

### Users and auth
- [x] Human users: first name, last name, display name, email, avatar, cover image
- [x] OIDC login (authorization code + PKCE, nonce-checked ID token). Users are matched by issuer+subject, then by verified email, and created on first login. Tested end to end against a mock provider.
- [x] Email/password login and registration (argon2), switchable with `PASSWORD_LOGIN` and `ALLOW_REGISTRATION`
- [x] Sessions: random tokens in an HttpOnly, SameSite=Lax cookie; only SHA-256 hashes are stored
- [x] Bot users: a name, belonging to one workspace, created by workspace admins
- [x] Bot access tokens (`mth_…`): shown once, hashed at rest, revocable, with last-used tracking

### MCP (bots)
- [x] `POST /mcp`: Streamable HTTP transport with stateless JSON responses. Supports `initialize`, `ping` and `tools/list`, plus `tools/call` with structured content and `isError` for tool failures.
- [x] Tools: `whoami`, `list_workspaces`, `list_projects`, `get_project` (with labels, epics and assignable members), `list_work_items` (with filters), `get_work_item` (with comments), `create_work_item`, `update_work_item`, `add_comment`, `add_relation`, `add_link`, `create_epic`
- [x] Projects and items can be referred to by identifier and key (`MTH`, `MTH-12`).
- [x] Bots write descriptions and comments as Markdown and read them back as Markdown. The server converts to and from the stored document, so bots never deal with the tree.

### Object storage
- [x] Uploads go through the API into any S3-compatible bucket (`object_store`), or to local disk when `S3_BUCKET` is unset. Tested against SeaweedFS's S3 gateway.
- [x] Used for project covers and icons, user avatars and covers, and work item attachments
- [x] Downloads go through the API with auth. Only images and PDFs render inline; everything else downloads, with `nosniff`, so an uploaded HTML or SVG file can't run script on our origin.

### Web app
- [x] Login page: OIDC button when configured, plus email sign-in and registration
- [x] **Projects list** (main view): cards with cover, icon, Project ID, description, a progress bar by state (hover or tap a segment for a popover such as "In Progress · 3 · 27%"), and counts; New project dialog (with the Project ID suggested from the name); workspace switcher and creation
- [x] **Project board** (main project view): one kanban column per state, with HTML5 drag and drop between and within columns (optimistic, persisted). Cards show key, priority, labels, epic, due date (red when overdue), counts, and the assignee avatar with a bot badge.
  - Filters: text, assignee (me / unassigned / anyone), priority, epic, label, and show cancelled (hidden by default)
  - Quick-add per column
- [x] **Work item panel** (drawer, deep-linkable via `&item=MTH-12`): inline title; state, priority, assignee, epic, dates and parent; label toggles; a rich text description; reactions; sub-items (with quick add); relations; links; attachments (upload or drag-drop, with image thumbnails); comments (rich text, ⌘↵ to send, edit and delete, bot badge); delete
  - Rich text uses a TipTap editor:
    - toolbar with active states; inline link field that accepts only web and email links (⌘K);
    - Markdown-style shortcuts (`## `, `- `, `> `, ``` ``` ```, `**bold**`), and pasted content is reduced to the schema;
    - read-only text is rendered straight from the document as React elements (`@tiptap/static-renderer`), with no `innerHTML`.
- [x] Project settings: details, cover and icon, labels, epics, delete (admins)
- [x] Settings: profile (names, avatar, cover, password); members (add by email, roles, remove, rename workspace); bots and tokens, with a ready-to-paste MCP config and a `claude mcp add` command
- [x] Light and dark themes (gui-mellow ThemeToggle)
- [x] Frosted-glass surfaces throughout (header, drawer, dialogs, menus, board columns and cards, inputs), tinted with highlight colours: each column takes its state's colour, the item drawer its item's state, everything else primary. Cards glow in that colour under the pointer.

## Decisions and notes

- **SPA and routing**: query-string routes keep the static export simple, since static export can't pre-render unknown dynamic `[id]` segments. Actix serves `index.html` for unknown paths.
- **Enumerations** are `text` columns with `CHECK` constraints rather than Postgres enum types, which makes them easy to extend in a migration and simple to map in Rust.
- **Bots are users** (`users.kind = 'bot'`). Assignment, "created by", comments and reactions work the same for both kinds, and bots get exactly their workspace's permissions.
- **Authorization**: any member of a workspace can read and edit its projects and items. Owners and admins also manage members, bots and tokens, and delete projects. Records outside your workspaces return 404.
- **Files**: any signed-in user can fetch a file by its (unguessable, v4 UUID) id. Tightening this to workspace-scoped access is on the list below.
- **Rich text is stored as a document tree, not HTML.** Work item descriptions (`jsonb`, NULL for none) and comments (`jsonb`) hold TipTap's JSON, the ProseMirror document.
  - **One schema, two halves.** The web defines it in `web/lib/richtext.ts`: StarterKit, with headings limited to levels 2–3 and links limited to http, https and mailto. The server's `richtext.rs` mirrors it, and adding a node or mark means changing both.
  - **Validation** rebuilds each incoming document from known nodes, marks and attributes only:
    - unknown or misplaced nodes are a 400 error, since that means the client and server disagree about the schema;
    - unsafe links lose their mark but keep their text;
    - empty trailing paragraphs are trimmed, and a document with no content is stored as NULL;
    - nesting depth and node count are capped.
  - **Output is rendered from the tree.** The web renders React elements, and bots get Markdown. Because there's never stored markup, there's nothing to sanitize on the way out, and other formats (plain text for search, email HTML for notifications) can be added the same way.
  - **Input** may be a document or a Markdown string, on REST and MCP alike. Markdown goes through pulldown-cmark, and single newlines become line breaks, as people expect in comments.
  - **Old HTML.** Migration `…0002` renamed the HTML columns to `legacy_*_html`, and on start-up the server converts any remaining rows using html5ever's tokenizer, then clears them. A later migration can drop those columns once every deployment has started once.
  - **Saving on blur.** The editor reports a change only if its own JSON changed since it last loaded. Stored documents can't be compared with the editor's JSON directly, because `jsonb` reorders keys and the server drops default attributes.
  - The editor is a stand-in until gui-mellow ships `RichTextEditor` and `RichText` (GM-13).
- **Frosted glass** lives in `web/styles/_frost.scss`:
  - `frost` is the full effect: backdrop blur and saturation, a tinted sheen, a bright top edge and a tinted rim.
  - `tile` is the same look without blur, for elements repeated many times (cards, inputs), where hundreds of blur layers would cost frame rate.
  - `bead` is for tinted pills, and `glow` is the pointer-following highlight (set by `GlowTracker`).
  - Faint colour washes on `body` give the glass something to frost.
  - gui-mellow's own controls turn to glass through a translucent `--gm-color-surface`.
  - With "reduce transparency" set in the OS, surfaces go solid.
- **Stacked bar popover** (`web/components/StackedBar.tsx`) is a stand-in until gui-mellow ships a stacked Meter/Stat mode (GM-11) and a Popover component (GM-12); swap it for those then.
  - It uses the native Popover API, so the popover renders in the top layer and escapes the card's `overflow: hidden`.
  - The bar sits inside the card's link, where focusable children aren't allowed, so keyboard and screen-reader users get the full breakdown as text instead.
  - On touch, the first tap on a segment shows its popover and a second tap opens the project.
- **TipTap** is pinned to 3.31.4 rather than the newest release, to stay a few weeks behind the latest publish.
- **History lives in one `activity` table**, written by the Rust code (not database triggers) inside each change's transaction.
  - Why not triggers: they'd log row noise (`sort_order`, `updated_at`) instead of intent, and they don't know who's acting.
  - `changes` holds `{field: {from, to}}`, plus readable `*_label`s for ids (assignee name, epic name, label names), so entries read well even after renames. Rich text is turned into excerpts for feeds.
  - Ids for project, item and entity are deliberately not foreign keys, so history outlives what it describes. Actor names are kept as snapshots.
- **Undo reapplies old values through the normal update path**, so it's validated, permission-checked and recorded like any other change (`action: "reverted"`, linked both ways). It only applies if each field still holds the value the entry set, so it never overwrites a newer change. Creating items, and anything outside items and comments, isn't undoable. That was the scope we chose; projects, labels and the rest are history-only.
- **Soft delete through views.** `work_items` and `comments` are now views over `work_items_all` and `comments_all` that hide deleted rows. Simple views are automatically updatable in Postgres, so every existing query and write works unchanged.
  - Catch: `SELECT *` fixes a view's columns when it's created, so a migration that adds a column to a `*_all` table must recreate its view.
  - Deleting a project is still permanent; only its history entry remains.
- **Real-time uses SSE, not WebSockets**, because all writes already go over REST and only server → browser traffic is needed.
  - **Fan-out:** each change calls `pg_notify` in its transaction, so the event is only sent if the change commits. Every server instance `LISTEN`s and re-broadcasts to its streams, so this works behind a load balancer with no Redis.
  - **Payload:** events say what changed, and clients re-fetch what they show, so they can't drift from the server. After a reconnect or a lagging stream, clients are told to `resync`.
  - **Membership** is re-checked before every event is sent.
  - **Edits in progress win:** refreshes never overwrite a title or description someone is typing, and the board waits while a card is being dragged.
  - **`Cache-Control: no-transform`** is required on the stream. Without it, compressing proxies (including the Next dev server) gzip and buffer it.
  - Reordering cards sends a live event but writes no history entry.
- **gui-mellow** must be installed from GitHub (as its README says), not as a `file:` link: Bun's file links nest the library's own `node_modules` (a second React), which breaks resolution.

## Next steps

- [ ] Trash retention: permanently purge items deleted more than N days ago (and their files)
- [ ] Presence and typing indicators ("Olive is viewing"); this is when a WebSocket channel would pay off, and collaborative editing (Yjs) would want one too
- [ ] MCP `undo` tool for bots, and record profile changes, which aren't tied to a workspace so they aren't in any feed today
- [ ] Workspace-scoped file access and S3 presigned URLs for large downloads and uploads
- [ ] Notifications (assigned to you, mentioned, comments on your items)
- [ ] @mentions in comments (a TipTap mention node, plus a matching node in `richtext.rs`); reactions on comments
- [ ] Drop the `legacy_*_html` columns in a follow-up migration
- [ ] Better pickers: searchable assignee, parent and relation comboboxes (there's a `gui-mellow-combobox` sibling repo), plus keyboard shortcuts
- [ ] Views beyond the board: list/table view, my work across projects, an epic roadmap by start and due dates
- [ ] Email invitations for people who haven't signed in yet
- [ ] More MCP: delete/archive, label management, attachments, and MCP resources for projects
- [ ] Tests: Rust integration tests against a throwaway database, plus Playwright flows for the board
- [ ] Deployment: Dockerfile (multi-stage: Bun build, then Rust build, then a slim runtime image)
