import { Button } from "gui-mellow/Button";
import { Credit } from "gui-mellow/Credit";
import { CtaBanner } from "gui-mellow/CtaBanner";
import { Eyebrow } from "gui-mellow/Eyebrow";
import { Feature, FeatureGrid } from "gui-mellow/FeatureGrid";
import { Hero } from "gui-mellow/Hero";
import { Section } from "gui-mellow/Section";
import { SiteFooter } from "gui-mellow/SiteFooter";
import { SiteHeader } from "gui-mellow/SiteHeader";
import { Stat } from "gui-mellow/Stat";
import { Steps } from "gui-mellow/Steps";
import { Terminal } from "gui-mellow/Terminal";
import { ThemeToggle } from "gui-mellow/ThemeToggle";
import { BoardPreview } from "@/components/BoardPreview";
import { BotIcon, Logo } from "@/components/Logo";
import styles from "./page.module.scss";

const REPO = "https://github.com/Mellow-Technology/methods";

const MCP_TOOLS = [
  "whoami",
  "list_workspaces",
  "list_projects",
  "get_project",
  "list_work_items",
  "get_work_item",
  "create_work_item",
  "update_work_item",
  "add_comment",
  "add_relation",
  "add_link",
  "create_epic",
  "get_activity",
];

const ACTIVITY = [
  { who: "Claude", bot: true, what: <>moved <b>MTH-12</b> from Todo to In Progress</>, when: "just now" },
  { who: "Claude", bot: true, what: <>commented on <b>MTH-12</b></>, when: "1m" },
  { who: "Olive", what: <>assigned <b>MTH-21</b> to Claude</>, when: "4m" },
  { who: "Kyle", what: <>undid a change to <b>MTH-18</b> priority</>, when: "9m" },
];

function Brand() {
  return (
    <a href="#top" className={styles.brand}>
      <Logo />
      Methods
    </a>
  );
}

export default function Home() {
  return (
    <>
      <SiteHeader
        brand={<Brand />}
        items={[
          { href: "#features", label: "Features" },
          { href: "#bots", label: "Bots & MCP" },
          { href: "#performance", label: "Performance" },
          { href: "#security", label: "Security" },
          { href: "#self-host", label: "Self-host" },
        ]}
        actions={
          <>
            <ThemeToggle />
            <Button href={REPO} variant="secondary" size="sm" icon={<GitHubIcon />}>
              GitHub
            </Button>
          </>
        }
      />

      <main id="top">
        <Hero
          size="full"
          align="center"
          pattern="none"
          eyebrow="Open source · Self-hosted · AGPL-3.0"
          title={
            <>
              Project management for people <em>and their bots</em>
            </>
          }
          subtitle="Kanban boards, rich text, full history and live updates, with a built-in MCP server so your AI agents pick up work, comment and ship alongside your team."
          actions={
            <>
              <Button href="#self-host" size="lg">
                Run it yourself
              </Button>
              <Button href={REPO} variant="secondary" size="lg" icon={<GitHubIcon />}>
                View on GitHub
              </Button>
            </>
          }
        >
          <BoardPreview />
        </Hero>

        <Section
          id="features"
          size="wide"
          eyebrow="Features"
          title="Everything a team needs, nothing it doesn't"
          description="Workspaces hold projects, projects hold work items, and every change is recorded."
        >
          <FeatureGrid min="320px">
            <Feature index="01" title="Kanban boards">
              One column per state, drag and drop between them, and filters for assignee, priority, epic and label.
              Every item gets a key like <code>MTH-12</code> and a deep link.
            </Feature>
            <Feature index="02" title="Rich work items">
              Assignees, priorities, dates, parents and sub-items, labels, epics, relations (blocks, blocked by,
              duplicates), links, attachments and emoji reactions.
            </Feature>
            <Feature index="03" title="Rich text, safely">
              Descriptions and comments are edited with Markdown shortcuts and stored as a validated document tree,
              never as HTML, with every version kept and restorable.
            </Feature>
            <Feature index="04" title="History and undo">
              Every change in a workspace writes an audit entry in the same transaction. Undo any field change, and
              restore deleted items and comments from the activity feed.
            </Feature>
            <Feature index="05" title="Live everywhere">
              Boards, panels and settings follow changes from other people, other tabs and bots as they happen. No
              refresh button.
            </Feature>
            <Feature index="06" title="Your storage">
              Covers, avatars and attachments go to any S3-compatible bucket (AWS, R2, MinIO, Garage, SeaweedFS) or
              to local disk.
            </Feature>
          </FeatureGrid>
        </Section>

        <Section
          id="bots"
          size="wide"
          eyebrow="Bots & MCP"
          title="Bots are teammates, not integrations"
          description="Methods speaks the Model Context Protocol out of the box. Give an agent a token and it can read the board, pick up work and report back, with nothing else to install."
        >
          <div className={styles.bots}>
            <div className={styles.botsCopy}>
              <ul className={styles.points}>
                <li>
                  <strong>Built-in MCP server.</strong> Streamable HTTP at <code>/mcp</code>, on the same origin as
                  the app. Works with Claude Code and any MCP client that speaks Streamable HTTP.
                </li>
                <li>
                  <strong>Real users.</strong> Bots can be assigned work, create items, comment and react, and every
                  change is attributed to them in the activity feed.
                </li>
                <li>
                  <strong>Same rules as everyone.</strong> Bots go through the same validation and permission checks
                  as people, scoped to their own workspace.
                </li>
                <li>
                  <strong>Markdown in, Markdown out.</strong> Agents write and read descriptions and comments as
                  Markdown; the server handles the rich text.
                </li>
                <li>
                  <strong>Revocable tokens.</strong> Shown once, hashed at rest, with last-used tracking.
                </li>
              </ul>
              <ul className={styles.tools} aria-label="MCP tools">
                {MCP_TOOLS.map((t) => (
                  <li key={t}>
                    <code>{t}</code>
                  </li>
                ))}
              </ul>
            </div>
            <div className={styles.botsDemo}>
              <Terminal
                label="Connect Claude Code"
                lines={[
                  "claude mcp add --transport http methods \\",
                  "  https://methods.example.com/mcp \\",
                  '  --header "Authorization: Bearer mth_…"',
                ]}
              />
              <div className={styles.feed}>
                <Eyebrow>Activity · MTH</Eyebrow>
                <ul className={styles.feedList}>
                  {ACTIVITY.map((a, i) => (
                    <li key={i}>
                      <span className={a.bot ? `${styles.who} ${styles.whoBot}` : styles.who} aria-hidden>
                        {a.bot ? <BotIcon size={13} /> : a.who[0]}
                      </span>
                      <span>
                        <strong>{a.who}</strong> {a.what}
                      </span>
                      <time>{a.when}</time>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        </Section>

        <Section
          id="performance"
          size="wide"
          eyebrow="Performance"
          title="Rust on the server, nothing extra to run"
          description="One small binary serves the app, the API and the MCP endpoint. All it needs is PostgreSQL."
        >
          <div className={styles.stats}>
            <Stat label="Server binary for app, API and MCP" value="1" />
            <Stat label="Live updates to every open board" value="~300" unit="ms" />
            <Stat label="Redis, queues or workers to run" value="0" />
          </div>
          <FeatureGrid min="300px">
            <Feature title="Rust and Actix Web">
              A compiled, async server on Tokio with sqlx and PostgreSQL. Migrations are embedded and run on start.
            </Feature>
            <Feature title="Real time without the plumbing">
              Changes fan out with Postgres <code>NOTIFY</code> and stream to browsers over Server-Sent Events, so
              live updates work across instances behind a load balancer.
            </Feature>
            <Feature title="A static front end">
              The web app is a static export served by the same binary. No Node.js in production, no server
              rendering, and pages ship CSS only for the components they use.
            </Feature>
          </FeatureGrid>
        </Section>

        <Section
          id="security"
          size="wide"
          eyebrow="Security"
          title="Single sign-on and sensible defaults"
          description="Sign in with the identity provider you already run. Authentik, Keycloak, Okta or any other OpenID Connect provider."
        >
          <FeatureGrid min="240px">
            <Feature title="OpenID Connect">
              Authorization code flow with PKCE and nonce-checked ID tokens. Accounts are matched by issuer and
              subject, then verified email, and created on first sign-in.
            </Feature>
            <Feature title="Hashed secrets">
              Session cookies are HttpOnly and SameSite; only SHA-256 hashes of sessions and bot tokens are stored.
              Optional passwords use Argon2.
            </Feature>
            <Feature title="Workspace roles">
              Owners, admins and members. Records outside your workspaces simply don&apos;t exist as far as you can
              tell.
            </Feature>
            <Feature title="Safe uploads">
              Only images and PDFs render inline. Everything else downloads with <code>nosniff</code>, so an uploaded
              file can&apos;t run script on your origin.
            </Feature>
          </FeatureGrid>
        </Section>

        <Section
          id="self-host"
          size="wide"
          eyebrow="Self-host"
          title="Up and running in one container"
          description="The image exposes a single port for the app, the API and the MCP endpoint."
        >
          <div className={styles.install}>
            <Steps
              steps={[
                { title: "Bring PostgreSQL", body: "Methods runs its own migrations on start, so an empty database is all it needs." },
                {
                  title: "Run the image",
                  body: "Set the database URL and the public URL people use. Add S3 and OIDC settings when you want them.",
                },
                {
                  title: "Invite your team, then your bots",
                  body: "Create a workspace, then a bot and a token under Settings → Bots & MCP.",
                },
              ]}
            />
            <Terminal
              label="docker"
              lines={[
                "docker run -p 8080:8080 \\",
                "  -e DATABASE_URL=postgres://user:pass@db/methods \\",
                "  -e PUBLIC_URL=https://methods.example.com \\",
                "  -e OIDC_ISSUER=https://auth.example.com/ \\",
                "  -e OIDC_CLIENT_ID=methods \\",
                "  -v methods-data:/data \\",
                "  ghcr.io/mellow-technology/methods",
              ]}
            />
          </div>
        </Section>

        <section className={styles.cta}>
          <CtaBanner
            title="Give your team and your agents the same board"
            actions={
              <>
                <Button href={REPO} icon={<GitHubIcon />}>
                  Get Methods
                </Button>
                <Button href={`${REPO}#readme`} variant="secondary">
                  Read the docs
                </Button>
              </>
            }
          >
            Free and open source under the AGPL-3.0. Your data stays on your servers.
          </CtaBanner>
        </section>
      </main>

      <SiteFooter
        brand={<Brand />}
        columns={[
          {
            heading: "Project",
            links: [
              { href: REPO, label: "Source code" },
              { href: `${REPO}/issues`, label: "Issues" },
              { href: `${REPO}/blob/main/LICENSE`, label: "Licence" },
            ],
          },
          {
            heading: "Get started",
            links: [
              { href: "#self-host", label: "Self-host" },
              { href: "#bots", label: "Connect a bot" },
              { href: `${REPO}/pkgs/container/methods`, label: "Container image" },
            ],
          },
        ]}
      >
        <p>Project management for people and their bots. © 2026 Mellow Technology.</p>
        <p>
          <Credit />
        </p>
      </SiteFooter>
    </>
  );
}

function GitHubIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden focusable="false" fill="currentColor">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}
