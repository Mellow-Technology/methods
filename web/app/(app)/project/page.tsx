"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { Alert } from "gui-mellow/Alert";
import { Button } from "gui-mellow/Button";
import { Drawer } from "gui-mellow/Drawer";
import { ActivityFeed } from "@/components/ActivityFeed";
import { Board } from "@/components/Board";
import { ItemPanel } from "@/components/ItemPanel";
import { NewItemDialog } from "@/components/NewItemDialog";
import { ProjectIcon } from "@/components/ProjectIcon";
import { ProjectSettingsDialog } from "@/components/ProjectSettingsDialog";
import {
  PRIORITIES,
  api,
  errorMessage,
  type Epic,
  type Label,
  type Member,
  type Project,
  type State,
  type WorkItem,
} from "@/lib/api";
import { useLiveRefresh } from "@/lib/live";
import { useSession } from "@/lib/session";
import styles from "./project.module.scss";

interface Filters {
  q: string;
  assignee: string;
  priority: string;
  epic: string;
  label: string;
  showCancelled: boolean;
}

const NO_FILTERS: Filters = { q: "", assignee: "", priority: "", epic: "", label: "", showCancelled: false };

function ProjectBoard() {
  const router = useRouter();
  const params = useSearchParams();
  const projectId = params.get("id");
  const itemKey = params.get("item");
  const { me, workspaces, workspace, selectWorkspace } = useSession();

  const [project, setProject] = useState<Project | null>(null);
  const [items, setItems] = useState<WorkItem[]>([]);
  const [labels, setLabels] = useState<Label[]>([]);
  const [epics, setEpics] = useState<Epic[]>([]);
  const [members, setMembers] = useState<Member[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  const [newItemState, setNewItemState] = useState<State | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activityOpen, setActivityOpen] = useState(false);

  const loadItems = useCallback(async () => {
    if (!projectId) return;
    const next = await api.get<WorkItem[]>(`/projects/${projectId}/work-items`);
    setItems((prev) => reuseUnchanged(prev, next));
  }, [projectId]);

  // ---- Live updates ----
  // A drop re-renders the columns; don't refresh under someone's drag.
  const dragging = useRef(false);
  useEffect(() => {
    const start = () => (dragging.current = true);
    const end = () => (dragging.current = false);
    document.addEventListener("dragstart", start);
    document.addEventListener("dragend", end);
    return () => {
      document.removeEventListener("dragstart", start);
      document.removeEventListener("dragend", end);
    };
  }, []);
  const loadMeta = useCallback(async () => {
    if (!projectId) return;
    try {
      const p = await api.get<Project>(`/projects/${projectId}`);
      const [ls, es, ms] = await Promise.all([
        api.get<Label[]>(`/projects/${projectId}/labels`),
        api.get<Epic[]>(`/projects/${projectId}/epics`),
        api.get<Member[]>(`/workspaces/${p.workspace_id}/members`),
      ]);
      setProject(p);
      setLabels(ls);
      setEpics(es);
      setMembers(ms);
    } catch (e) {
      setProject(null);
      setError(errorMessage(e));
    }
  }, [projectId]);
  useLiveRefresh(
    (e) => e.project_id === projectId && ["work_item", "comment", "attachment"].includes(e.entity_type),
    () => loadItems().catch(() => {}),
    { paused: () => dragging.current },
  );
  useLiveRefresh(
    (e) =>
      (e.project_id === projectId && ["project", "label", "epic"].includes(e.entity_type)) ||
      ["member", "bot"].includes(e.entity_type),
    loadMeta,
  );

  useEffect(() => {
    if (!projectId) return;
    let cancelled = false;
    (async () => {
      try {
        const p = await api.get<Project>(`/projects/${projectId}`);
        const [it, ls, es, ms] = await Promise.all([
          api.get<WorkItem[]>(`/projects/${projectId}/work-items`),
          api.get<Label[]>(`/projects/${projectId}/labels`),
          api.get<Epic[]>(`/projects/${projectId}/epics`),
          api.get<Member[]>(`/workspaces/${p.workspace_id}/members`),
        ]);
        if (cancelled) return;
        setProject(p);
        setItems(it);
        setLabels(ls);
        setEpics(es);
        setMembers(ms);
      } catch (e) {
        if (!cancelled) setError(errorMessage(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  useEffect(() => {
    if (project) document.title = `${project.name} · Methods`;
  }, [project]);

  // Opening a project from another workspace switches to it, so the live
  // stream (one per workspace) covers what's on screen.
  useEffect(() => {
    if (project && workspace && project.workspace_id !== workspace.id) selectWorkspace(project.workspace_id);
  }, [project, workspace, selectWorkspace]);

  const setItemParam = useCallback(
    (key: string | null) => {
      const q = new URLSearchParams(params.toString());
      if (key) q.set("item", key);
      else q.delete("item");
      router.replace(`/project/?${q.toString()}`, { scroll: false });
    },
    [params, router],
  );

  const labelMap = useMemo(() => new Map(labels.map((l) => [l.id, l])), [labels]);
  const epicMap = useMemo(() => new Map(epics.map((e) => [e.id, e])), [epics]);

  // The board follows the filters a beat behind, so typing in the search box
  // never waits on re-rendering the columns.
  const applied = useDeferredValue(filters);
  const visible = useMemo(() => {
    const f = applied;
    const q = f.q.trim().toLowerCase();
    return items.filter((i) => {
      if (q && !i.title.toLowerCase().includes(q) && !i.key.toLowerCase().includes(q)) return false;
      if (f.assignee === "me" && i.assignee_id !== me.id) return false;
      if (f.assignee === "none" && i.assignee_id) return false;
      if (f.assignee && !["me", "none"].includes(f.assignee) && i.assignee_id !== f.assignee) return false;
      if (f.priority && i.priority !== f.priority) return false;
      if (f.epic && i.epic_id !== f.epic) return false;
      if (f.label && !i.label_ids.includes(f.label)) return false;
      return true;
    });
  }, [items, applied, me.id]);

  const onChanged = useCallback((id: string, updated: WorkItem | null) => {
    setItems((list) => {
      if (!updated) return list.filter((i) => i.id !== id);
      const exists = list.some((i) => i.id === id);
      return exists ? list.map((i) => (i.id === id ? { ...i, ...pickSummary(updated) } : i)) : [...list, pickSummary(updated)];
    });
  }, []);

  async function move(item: WorkItem, state: State, sortOrder: number) {
    const previous = items;
    setItems((list) => list.map((i) => (i.id === item.id ? { ...i, state, sort_order: sortOrder } : i)));
    try {
      await api.patch(`/work-items/${item.id}`, { state, sort_order: sortOrder });
    } catch (e) {
      setItems(previous);
      setError(errorMessage(e));
    }
  }

  if (!projectId) return <Alert tone="warning">No project selected.</Alert>;
  if (error && !project) {
    return (
      <div className={styles.message}>
        <Alert tone="danger" title="Couldn't open this project">
          {error}
        </Alert>
        <Button href="/" variant="secondary">
          Back to projects
        </Button>
      </div>
    );
  }
  if (!project) return <div className="methods-loading">Loading board…</div>;

  const role = workspaces.find((w) => w.id === project.workspace_id)?.role;
  const filtered = JSON.stringify({ ...filters, showCancelled: false }) !== JSON.stringify({ ...NO_FILTERS });
  const set = (k: keyof Filters) => (e: { target: { value: string } }) => setFilters((f) => ({ ...f, [k]: e.target.value }));

  return (
    <main className={styles.page} data-fill-viewport>
      <header className={styles.header}>
        <div
          className={styles.cover}
          style={project.cover_url ? { backgroundImage: `url(${project.cover_url})` } : undefined}
        />
        <div className={styles.headerBody}>
          <ProjectIcon project={project} size={56} className={styles.icon} />
          <div className={styles.titles}>
            <p className={styles.crumbs}>
              <Link href="/">Projects</Link> / <span className={styles.ident}>{project.identifier}</span>
            </p>
            <h1>{project.name}</h1>
            {project.description && <p className={styles.description}>{project.description}</p>}
          </div>
          <div className={styles.headerActions}>
            <Button variant="secondary" onClick={() => setActivityOpen(true)}>
              Activity
            </Button>
            <Button variant="secondary" onClick={() => setSettingsOpen(true)}>
              Settings
            </Button>
            <Button onClick={() => setNewItemState("backlog")}>New item</Button>
          </div>
        </div>
      </header>

      <div className={styles.toolbar} role="search">
        <input className={styles.search} type="search" placeholder="Filter by title or key…" value={filters.q} onChange={set("q")} aria-label="Filter" />
        <select className={styles.filter} value={filters.assignee} onChange={set("assignee")} aria-label="Assignee">
          <option value="">Anyone</option>
          <option value="me">Assigned to me</option>
          <option value="none">Unassigned</option>
          {members.map((m) => (
            <option key={m.id} value={m.id}>
              {m.kind === "bot" ? "🤖 " : ""}
              {m.display_name}
            </option>
          ))}
        </select>
        <select className={styles.filter} value={filters.priority} onChange={set("priority")} aria-label="Priority">
          <option value="">Any priority</option>
          {PRIORITIES.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
        {epics.length > 0 && (
          <select className={styles.filter} value={filters.epic} onChange={set("epic")} aria-label="Epic">
            <option value="">Any epic</option>
            {epics.map((e) => (
              <option key={e.id} value={e.id}>
                {e.name}
              </option>
            ))}
          </select>
        )}
        {labels.length > 0 && (
          <select className={styles.filter} value={filters.label} onChange={set("label")} aria-label="Label">
            <option value="">Any label</option>
            {labels.map((l) => (
              <option key={l.id} value={l.id}>
                {l.name}
              </option>
            ))}
          </select>
        )}
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={filters.showCancelled}
            onChange={(e) => setFilters((f) => ({ ...f, showCancelled: e.target.checked }))}
          />
          Show cancelled
        </label>
        {filtered && (
          <Button size="sm" variant="ghost" onClick={() => setFilters((f) => ({ ...NO_FILTERS, showCancelled: f.showCancelled }))}>
            Clear filters
          </Button>
        )}
        <span className={styles.total}>
          {visible.length === items.length ? `${items.length} items` : `${visible.length} of ${items.length} items`}
        </span>
      </div>

      {error && (
        <div className={styles.inlineError}>
          <Alert tone="danger">{error}</Alert>
        </div>
      )}

      <Board
        items={visible}
        labels={labelMap}
        epics={epicMap}
        selectedKey={itemKey}
        hiddenStates={filters.showCancelled ? [] : ["cancelled"]}
        onOpen={(i) => setItemParam(i.key)}
        onMove={move}
        onAdd={setNewItemState}
      />

      <ItemPanel
        itemKey={itemKey}
        projectItems={items}
        labels={labels}
        epics={epics}
        members={members}
        onClose={() => setItemParam(null)}
        onOpen={setItemParam}
        onChanged={(id, updated) => {
          onChanged(id, updated);
          // Sub-items created from the panel aren't in the list yet.
          if (updated && updated.child_count !== items.find((i) => i.id === id)?.child_count) loadItems();
        }}
      />

      <Drawer
        open={activityOpen}
        onClose={() => setActivityOpen(false)}
        title="Activity"
        subtitle={`Everything that happened in ${project.name}`}
      >
        {activityOpen && (
          <ActivityFeed
            url={`/projects/${project.id}/activity`}
            matches={(e) => e.id !== null && e.project_id === project.id}
            meId={me.id}
            showTarget
            onOpenItem={(key) => {
              setActivityOpen(false);
              setItemParam(key);
            }}
            onReverted={() => loadItems()}
          />
        )}
      </Drawer>

      <NewItemDialog
        projectId={project.id}
        open={newItemState !== null}
        initialState={newItemState ?? "backlog"}
        members={members}
        epics={epics}
        onClose={() => setNewItemState(null)}
        onCreated={(item, keepOpen) => {
          onChanged(item.id, item);
          if (!keepOpen) {
            setNewItemState(null);
            setItemParam(item.key);
          }
        }}
      />

      <ProjectSettingsDialog
        open={settingsOpen}
        project={project}
        labels={labels}
        epics={epics}
        canDelete={role === "owner" || role === "admin"}
        onClose={() => setSettingsOpen(false)}
        onProject={(p) => {
          setProject(p);
          if (p.identifier !== project.identifier) loadItems();
        }}
        onLabels={setLabels}
        onEpics={setEpics}
      />
    </main>
  );
}

function sameItem(a: WorkItem, b: WorkItem) {
  const ka = Object.keys(a) as (keyof WorkItem)[];
  if (ka.length !== Object.keys(b).length) return false;
  return ka.every((k) => {
    const x = a[k];
    const y = b[k];
    if (Array.isArray(x) && Array.isArray(y)) return x.length === y.length && x.every((v, i) => v === y[i]);
    return x === y;
  });
}

/**
 * A refetched list with the old objects kept wherever nothing changed, so
 * memoised cards skip re-rendering. Returns `prev` itself when the whole list
 * is unchanged, so a live refresh that changed nothing renders nothing.
 */
function reuseUnchanged(prev: WorkItem[], next: WorkItem[]): WorkItem[] {
  const old = new Map(prev.map((i) => [i.id, i]));
  let identical = prev.length === next.length;
  const merged = next.map((n, index) => {
    const o = old.get(n.id);
    if (o && sameItem(o, n)) {
      if (prev[index] !== o) identical = false;
      return o;
    }
    identical = false;
    return n;
  });
  return identical ? prev : merged;
}

/** Keeps only the fields the board holds, so detail payloads don't pile up in state. */
function pickSummary(i: WorkItem): WorkItem {
  const {
    id, project_id, seq, key, title, state, priority, assignee_id, assignee_name, assignee_kind, assignee_avatar_url,
    created_by, start_date, due_date, parent_id, epic_id, sort_order, label_ids, comment_count, attachment_count,
    child_count, created_at, updated_at,
  } = i;
  return {
    id, project_id, seq, key, title, state, priority, assignee_id, assignee_name, assignee_kind, assignee_avatar_url,
    created_by, start_date, due_date, parent_id, epic_id, sort_order, label_ids, comment_count, attachment_count,
    child_count, created_at, updated_at,
  };
}

export default function ProjectPage() {
  return (
    <Suspense fallback={<div className="methods-loading">Loading board…</div>}>
      <ProjectBoard />
    </Suspense>
  );
}
