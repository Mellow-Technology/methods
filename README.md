# Methods

Project management for people and their bots. Workspaces hold projects;
projects hold work items on a kanban board. Humans sign in with OIDC (or
email/password for testing); bots connect over MCP with access tokens.

See [Methods.md](Methods.md) for the plan and the current status.

```
server/   Rust · Actix Web · sqlx/PostgreSQL · S3-compatible storage · MCP endpoint
web/      Next.js (static export SPA) · gui-mellow
```

## Running locally

Needs Rust 1.87+, Bun, and PostgreSQL.

```sh
createdb methods

# API on :8080 (runs migrations on start)
cd server && cargo run

# Web app on :3000, proxying /api and /mcp to :8080
cd web && bun install && bun dev
```

Open http://localhost:3000, create an account, then a workspace and a project.

Settings live in environment variables or `server/.env`; see
[`server/.env.example`](server/.env.example). Without `S3_BUCKET` uploads go to
`server/data/files`. To try S3 and OIDC locally, start the services in
[`docker-compose.dev.yml`](docker-compose.dev.yml).

## Production build

```sh
cd web && bun run build          # static export to web/out
cd server && cargo build --release
PUBLIC_URL=https://methods.example.com STATIC_DIR=../web/out ./target/release/methods-server
```

The Rust server then hosts the SPA, `/api` and `/mcp` on one origin.

## Connecting a bot over MCP

In **Settings → Bots & MCP**, create a bot and an access token. Then point an
MCP client at `${PUBLIC_URL}/mcp` with the token as a bearer token, e.g.

```sh
claude mcp add --transport http methods http://localhost:3000/mcp \
  --header "Authorization: Bearer mth_…"
```

Tools: `whoami`, `list_workspaces`, `list_projects`, `get_project`,
`list_work_items`, `get_work_item`, `create_work_item`, `update_work_item`,
`add_comment`, `add_relation`, `add_link`, `create_epic`. They run with the
bot's own workspace permissions, and its changes are attributed to it.

## License

Methods is licensed under the [GNU Affero General Public License v3.0](LICENSE).
