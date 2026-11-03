"use client";

import Link from "next/link";
import { useState, type ReactNode } from "react";
import { Avatar } from "gui-mellow/Avatar";
import { Button } from "gui-mellow/Button";
import { Dialog } from "gui-mellow/Dialog";
import { TextField } from "gui-mellow/Field";
import { SiteHeader } from "gui-mellow/SiteHeader";
import { ThemeToggle } from "gui-mellow/ThemeToggle";
import { Logo } from "@/components/Logo";
import { api, errorMessage, type Workspace } from "@/lib/api";
import { LiveProvider, useLiveRefresh, useLiveStatus } from "@/lib/live";
import { SessionProvider, useSession } from "@/lib/session";
import styles from "./shell.module.scss";

function WorkspaceSwitcher() {
  const { workspaces, workspace, selectWorkspace, refreshWorkspaces } = useSession();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const ws = await api.post<Workspace>("/workspaces", { name });
      await refreshWorkspaces();
      selectWorkspace(ws.id);
      setCreating(false);
      setName("");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      {workspaces.length > 0 && (
        <select
          className={styles.switcher}
          aria-label="Workspace"
          value={workspace?.id ?? ""}
          onChange={(e) => (e.target.value === "__new" ? setCreating(true) : selectWorkspace(e.target.value))}
        >
          {workspaces.map((w) => (
            <option key={w.id} value={w.id}>
              {w.name}
            </option>
          ))}
          <option value="__new">+ New workspace…</option>
        </select>
      )}
      <Dialog
        open={creating}
        onClose={() => setCreating(false)}
        title="New workspace"
        description="Workspaces hold projects, people and bots."
        size="sm"
        actions={
          <>
            <Button variant="ghost" onClick={() => setCreating(false)}>
              Cancel
            </Button>
            <Button onClick={create} loading={busy} disabled={!name.trim()}>
              Create
            </Button>
          </>
        }
      >
        <TextField
          label="Name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          error={error ?? undefined}
          autoFocus
          onKeyDown={(e) => e.key === "Enter" && name.trim() && create()}
        />
      </Dialog>
    </>
  );
}

function UserMenu() {
  const { me, logout } = useSession();
  return (
    <details className={styles.menu}>
      <summary aria-label="Account">
        <Avatar name={me.display_name} src={me.avatar_url ?? undefined} size={30} />
      </summary>
      <div className={styles.menuPanel}>
        <p className={styles.menuName}>{me.display_name}</p>
        <p className={styles.menuEmail}>{me.email}</p>
        <Link href="/settings/" className={styles.menuItem}>
          Settings
        </Link>
        <button type="button" className={styles.menuItem} onClick={logout}>
          Sign out
        </button>
      </div>
    </details>
  );
}

/** A small dot: green while live updates are flowing. */
function LiveIndicator() {
  const status = useLiveStatus();
  const label = { live: "Live updates on", connecting: "Connecting for live updates…", offline: "Live updates off" }[status];
  return <span className={styles.live} data-status={status} title={label} aria-label={label} role="status" />;
}

/** Workspace-level changes (renames, membership) refresh the session. */
function WorkspaceSync() {
  const { refreshWorkspaces } = useSession();
  useLiveRefresh(
    (e) => e.entity_type === "workspace" || e.entity_type === "member" || e.entity_type === "project",
    () => refreshWorkspaces().catch(() => {}),
  );
  return null;
}

function Shell({ children }: { children: ReactNode }) {
  const { workspace } = useSession();
  return (
    <LiveProvider workspaceId={workspace?.id ?? null}>
      <WorkspaceSync />
      <ShellFrame>{children}</ShellFrame>
    </LiveProvider>
  );
}

function ShellFrame({ children }: { children: ReactNode }) {
  return (
    <div className={styles.shell}>
      <SiteHeader
        className={styles.header}
        brand={
          <span className={styles.brandRow}>
            <Link href="/" className={styles.brand}>
              <Logo size={26} />
              <span>Methods</span>
            </Link>
            <WorkspaceSwitcher />
          </span>
        }
        items={[
          { href: "/", label: "Projects" },
          { href: "/settings/", label: "Settings" },
        ]}
        actions={
          <span className={styles.actions}>
            <LiveIndicator />
            <ThemeToggle />
            <UserMenu />
          </span>
        }
      />
      <div className={styles.main}>{children}</div>
    </div>
  );
}

export default function AppLayout({ children }: { children: ReactNode }) {
  return (
    <SessionProvider fallback={<div className="methods-loading">Loading…</div>}>
      <Shell>{children}</Shell>
    </SessionProvider>
  );
}
