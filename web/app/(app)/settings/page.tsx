"use client";

import { useCallback, useEffect, useState } from "react";
import { Alert } from "gui-mellow/Alert";
import { Avatar } from "gui-mellow/Avatar";
import { Badge } from "gui-mellow/Badge";
import { Button } from "gui-mellow/Button";
import { CodeBlock } from "gui-mellow/CodeBlock";
import { Container } from "gui-mellow/Container";
import { ConfirmDialog, Dialog } from "gui-mellow/Dialog";
import { SelectField, TextField } from "gui-mellow/Field";
import { PageHeader } from "gui-mellow/PageHeader";
import { Tabs } from "gui-mellow/Tabs";
import { CoverField, CoverFieldError, CoverSlot } from "@/components/CoverField";
import { useImageUpload } from "@/components/useImageUpload";
import { BotIcon } from "@/components/Icons";
import { api, errorMessage, timeAgo, type Bot, type BotToken, type Member, type User } from "@/lib/api";
import { useLiveRefresh } from "@/lib/live";
import { useSession } from "@/lib/session";
import styles from "./settings.module.scss";

function useFlash() {
  const [message, setMessage] = useState<{ tone: "success" | "danger"; text: string } | null>(null);
  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    setMessage(null);
    try {
      await fn();
      if (ok) setMessage({ tone: "success", text: ok });
    } catch (e) {
      setMessage({ tone: "danger", text: errorMessage(e) });
    }
  };
  const node = message && <Alert tone={message.tone}>{message.text}</Alert>;
  return { run, node };
}

// ---- Profile ---------------------------------------------------------------

function Profile() {
  const { me, setMe } = useSession();
  const [form, setForm] = useState({
    first_name: me.first_name ?? "",
    last_name: me.last_name ?? "",
    display_name: me.display_name,
  });
  const [passwords, setPasswords] = useState({ current_password: "", new_password: "" });
  const { run, node } = useFlash();

  const save = (body: Record<string, unknown>, ok = "Saved") =>
    run(async () => setMe(await api.patch<User>("/me", body)), ok);
  const avatar = useImageUpload((id) => save({ avatar_file_id: id }, "Avatar updated"));

  return (
    <div className={styles.tab}>
      {node}
      <CoverField url={me.cover_url} onChange={(id) => save({ cover_file_id: id }, id ? "Cover updated" : "Cover removed")}>
        <CoverSlot label="Change avatar" round onClick={avatar.choose}>
          <Avatar name={me.display_name} src={me.avatar_url ?? undefined} size={72} />
        </CoverSlot>
        <div className={styles.avatarControls}>
          <Button size="sm" variant="secondary" loading={avatar.busy} onClick={avatar.choose}>
            {me.avatar_url ? "Replace avatar" : "Upload avatar"}
          </Button>
          {me.avatar_url && (
            <Button size="sm" variant="ghost" onClick={() => save({ avatar_file_id: null }, "Avatar removed")}>
              Remove
            </Button>
          )}
        </div>
        {avatar.error && <CoverFieldError>{avatar.error}</CoverFieldError>}
        {avatar.input}
      </CoverField>
      <form
        className={styles.form}
        onSubmit={(e) => {
          e.preventDefault();
          save({ ...form, first_name: form.first_name || null, last_name: form.last_name || null });
        }}
      >
        <div className={styles.row}>
          <TextField label="First name" value={form.first_name} onChange={(e) => setForm({ ...form, first_name: e.target.value })} />
          <TextField label="Last name" value={form.last_name} onChange={(e) => setForm({ ...form, last_name: e.target.value })} />
        </div>
        <TextField
          label="Display name"
          required
          hint="How you appear on work items and comments"
          value={form.display_name}
          onChange={(e) => setForm({ ...form, display_name: e.target.value })}
        />
        <TextField label="Email" value={me.email ?? ""} disabled hint="Your sign-in address" />
        <div>
          <Button type="submit">Save profile</Button>
        </div>
      </form>
      <form
        className={styles.form}
        onSubmit={(e) => {
          e.preventDefault();
          save(passwords, "Password changed").then(() => setPasswords({ current_password: "", new_password: "" }));
        }}
      >
        <h3>Password</h3>
        <div className={styles.row}>
          <TextField
            label="Current password"
            type="password"
            autoComplete="current-password"
            value={passwords.current_password}
            onChange={(e) => setPasswords({ ...passwords, current_password: e.target.value })}
          />
          <TextField
            label="New password"
            type="password"
            autoComplete="new-password"
            minLength={8}
            value={passwords.new_password}
            onChange={(e) => setPasswords({ ...passwords, new_password: e.target.value })}
          />
        </div>
        <div>
          <Button type="submit" variant="secondary" disabled={passwords.new_password.length < 8}>
            Change password
          </Button>
        </div>
      </form>
    </div>
  );
}

// ---- Members ---------------------------------------------------------------

function Members({ workspaceId, isAdmin }: { workspaceId: string; isAdmin: boolean }) {
  const { me, workspace, refreshWorkspaces } = useSession();
  const [members, setMembers] = useState<Member[]>([]);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState("member");
  const [name, setName] = useState(workspace?.name ?? "");
  const { run, node } = useFlash();

  useEffect(() => {
    api.get<Member[]>(`/workspaces/${workspaceId}/members`).then(setMembers);
    setName(workspace?.name ?? "");
  }, [workspaceId, workspace?.name]);
  useLiveRefresh(
    (e) => e.entity_type === "member" || e.entity_type === "bot",
    () => api.get<Member[]>(`/workspaces/${workspaceId}/members`).then(setMembers).catch(() => {}),
  );

  const humans = members.filter((m) => m.kind === "human");

  return (
    <div className={styles.tab}>
      {node}
      {isAdmin && (
        <form
          className={styles.inline}
          onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              await api.patch(`/workspaces/${workspaceId}`, { name });
              await refreshWorkspaces();
            }, "Workspace renamed");
          }}
        >
          <TextField label="Workspace name" value={name} onChange={(e) => setName(e.target.value)} className={styles.grow} />
          <Button type="submit" variant="secondary" disabled={!name.trim() || name === workspace?.name}>
            Rename
          </Button>
        </form>
      )}

      <ul className={styles.list}>
        {humans.map((m) => (
          <li key={m.id} className={styles.member}>
            <Avatar name={m.display_name} src={m.avatar_url ?? undefined} size={36} />
            <span className={styles.memberInfo}>
              <strong>
                {m.display_name}
                {m.id === me.id && " (you)"}
              </strong>
              <span>{m.email}</span>
            </span>
            {isAdmin && m.role !== "owner" ? (
              <select
                className={styles.roleSelect}
                aria-label={`Role for ${m.display_name}`}
                value={m.role}
                onChange={(e) =>
                  run(async () =>
                    setMembers(await api.patch<Member[]>(`/workspaces/${workspaceId}/members/${m.id}`, { role: e.target.value })),
                  )
                }
              >
                <option value="admin">Admin</option>
                <option value="member">Member</option>
              </select>
            ) : (
              <Badge tone={m.role === "owner" ? "primary" : "neutral"}>{m.role}</Badge>
            )}
            {isAdmin && m.role !== "owner" && m.id !== me.id && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  run(async () => {
                    await api.del(`/workspaces/${workspaceId}/members/${m.id}`);
                    setMembers((list) => list.filter((x) => x.id !== m.id));
                  })
                }
              >
                Remove
              </Button>
            )}
          </li>
        ))}
      </ul>

      {isAdmin && (
        <form
          className={styles.inline}
          onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              setMembers(await api.post<Member[]>(`/workspaces/${workspaceId}/members`, { email, role }));
              setEmail("");
            }, "Member added");
          }}
        >
          <TextField
            label="Add a member by email"
            type="email"
            hint="They need to have signed in once already"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className={styles.grow}
          />
          <SelectField
            label="Role"
            value={role}
            onChange={(e) => setRole(e.target.value)}
            options={[
              { value: "member", label: "Member" },
              { value: "admin", label: "Admin" },
            ]}
          />
          <Button type="submit" disabled={!email.trim()}>
            Add
          </Button>
        </form>
      )}
    </div>
  );
}

// ---- Bots ------------------------------------------------------------------

function mcpConfig(url: string, token: string) {
  return JSON.stringify(
    { mcpServers: { methods: { type: "http", url, headers: { Authorization: `Bearer ${token}` } } } },
    null,
    2,
  );
}

function BotTokens({ bot, onChange }: { bot: Bot; onChange: () => void }) {
  const [tokens, setTokens] = useState<BotToken[]>([]);
  const [created, setCreated] = useState<{ token: string; mcp_url: string } | null>(null);
  const { run, node } = useFlash();

  const load = useCallback(() => api.get<BotToken[]>(`/bots/${bot.id}/tokens`).then(setTokens), [bot.id]);
  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className={styles.tokens}>
      {node}
      {tokens.length > 0 && (
        <ul className={styles.list}>
          {tokens.map((t) => (
            <li key={t.id} className={`${styles.token} ${t.revoked_at ? styles.revoked : ""}`}>
              <code>{t.token_prefix}…</code>
              <span>{t.name}</span>
              <span className={styles.muted}>
                {t.revoked_at
                  ? `revoked ${timeAgo(t.revoked_at)}`
                  : t.last_used_at
                    ? `used ${timeAgo(t.last_used_at)}`
                    : "never used"}
              </span>
              {!t.revoked_at && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() =>
                    run(async () => {
                      await api.del(`/bot-tokens/${t.id}`);
                      await load();
                      onChange();
                    })
                  }
                >
                  Revoke
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      <div>
        <Button
          size="sm"
          variant="secondary"
          onClick={() =>
            run(async () => {
              const res = await api.post<{ token: string; mcp_url: string }>(`/bots/${bot.id}/tokens`, {
                name: `Token ${tokens.length + 1}`,
              });
              setCreated(res);
              await load();
              onChange();
            })
          }
        >
          New access token
        </Button>
      </div>
      <Dialog
        open={created !== null}
        onClose={() => setCreated(null)}
        title={`Access token for ${bot.display_name}`}
        description="Copy it now. It won't be shown again."
        size="lg"
        actions={<Button onClick={() => setCreated(null)}>Done</Button>}
      >
        {created && (
          <div className={styles.created}>
            <CodeBlock code={created.token} language="token" />
            <p>
              Point any MCP client at <code>{created.mcp_url}</code> with the token as a bearer token. For example, in
              an <code>.mcp.json</code>:
            </p>
            <CodeBlock code={mcpConfig(created.mcp_url, created.token)} language="json" />
            <p>Or with Claude Code:</p>
            <CodeBlock
              code={`claude mcp add --transport http methods ${created.mcp_url} --header "Authorization: Bearer ${created.token}"`}
              language="sh"
            />
          </div>
        )}
      </Dialog>
    </div>
  );
}

function Bots({ workspaceId, isAdmin }: { workspaceId: string; isAdmin: boolean }) {
  const [bots, setBots] = useState<Bot[]>([]);
  const [name, setName] = useState("");
  const [deleting, setDeleting] = useState<Bot | null>(null);
  const { run, node } = useFlash();
  const load = useCallback(() => api.get<Bot[]>(`/workspaces/${workspaceId}/bots`).then(setBots), [workspaceId]);

  useEffect(() => {
    load();
  }, [load]);
  useLiveRefresh((e) => e.entity_type === "bot" || e.entity_type === "bot_token", () => load().catch(() => {}));

  return (
    <div className={styles.tab}>
      {node}
      <p className={styles.intro}>
        Bots are workspace members that connect over the Model Context Protocol. They can read projects, create and
        update work items, and comment — and work can be assigned to them like anyone else.
      </p>
      {bots.length === 0 && <p className={styles.muted}>No bots yet.</p>}
      <ul className={styles.bots}>
        {bots.map((b) => (
          <li key={b.id} className={styles.bot}>
            <header className={styles.botHeader}>
              <span className={styles.botAvatar}>
                <BotIcon size={18} />
              </span>
              <span className={styles.memberInfo}>
                <strong>{b.display_name}</strong>
                <span>
                  {b.active_tokens} active token{b.active_tokens === 1 ? "" : "s"}
                  {b.last_used_at && ` · last seen ${timeAgo(b.last_used_at)}`}
                </span>
              </span>
              {isAdmin && (
                <Button size="sm" variant="ghost" onClick={() => setDeleting(b)}>
                  Delete
                </Button>
              )}
            </header>
            {isAdmin && <BotTokens bot={b} onChange={load} />}
          </li>
        ))}
      </ul>
      {isAdmin ? (
        <form
          className={styles.inline}
          onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              await api.post<Bot>(`/workspaces/${workspaceId}/bots`, { name });
              setName("");
              await load();
            });
          }}
        >
          <TextField label="New bot" placeholder="Claude" value={name} onChange={(e) => setName(e.target.value)} className={styles.grow} />
          <Button type="submit" disabled={!name.trim()}>
            Create bot
          </Button>
        </form>
      ) : (
        <p className={styles.muted}>Workspace admins can add bots.</p>
      )}
      <ConfirmDialog
        open={deleting !== null}
        title={`Delete ${deleting?.display_name}?`}
        destructive
        confirmLabel="Delete bot"
        onCancel={() => setDeleting(null)}
        onConfirm={() =>
          run(async () => {
            await api.del(`/bots/${deleting!.id}`);
            setDeleting(null);
            await load();
          })
        }
      >
        Its tokens stop working immediately. Work assigned to it becomes unassigned.
      </ConfirmDialog>
    </div>
  );
}

export default function SettingsPage() {
  const { workspace } = useSession();
  const isAdmin = workspace?.role === "owner" || workspace?.role === "admin";
  const tabs = [{ value: "profile", label: "Profile", content: <Profile /> }];
  if (workspace) {
    tabs.push(
      { value: "members", label: "Members", content: <Members workspaceId={workspace.id} isAdmin={isAdmin} /> },
      { value: "bots", label: "Bots & MCP", content: <Bots workspaceId={workspace.id} isAdmin={isAdmin} /> },
    );
  }
  return (
    <Container as="main" className={styles.page}>
      <PageHeader title="Settings" eyebrow={workspace?.name} />
      <Tabs label="Settings" tabs={tabs} />
    </Container>
  );
}
