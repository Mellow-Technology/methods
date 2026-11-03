"use client";

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Avatar } from "gui-mellow/Avatar";
import {
  PRIORITIES,
  RELATION_LABELS,
  api,
  errorMessage,
  formatDate,
  stateInfo,
  timeAgo,
  type ActivityEntry,
  type Change,
  type LiveEvent,
  type RelationView,
} from "@/lib/api";
import { useLiveRefresh } from "@/lib/live";
import { BotIcon } from "./Icons";
import styles from "./ActivityFeed.module.scss";

interface Props {
  /** e.g. `/work-items/{id}/activity` or `/projects/{id}/activity`. */
  url: string;
  /** Which live changes should refresh this feed. */
  matches: (e: LiveEvent) => boolean;
  meId: string;
  /** Name the item each entry is about (project feeds); item feeds say "it". */
  showTarget?: boolean;
  /** Opens a work item by key, from the "MTH-12" in an entry. */
  onOpenItem?: (key: string) => void;
  /** Description entries offer this ("view versions"). */
  onDescriptionHistory?: () => void;
  /** After an undo succeeds. */
  onReverted?: (entry: ActivityEntry) => void;
  pageSize?: number;
}

export function ActivityFeed({
  url,
  matches,
  meId,
  showTarget,
  onOpenItem,
  onDescriptionHistory,
  onReverted,
  pageSize = 30,
}: Props) {
  const [entries, setEntries] = useState<ActivityEntry[] | null>(null);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<number | null>(null);

  const firstPage = useRef(true);

  const load = useCallback(async () => {
    try {
      const list = await api.get<ActivityEntry[]>(`${url}?limit=${pageSize}`);
      // Keep any older pages already loaded.
      setEntries((prev) => {
        if (!prev || prev.length <= list.length) return list;
        const oldest = list.at(-1)?.id ?? Infinity;
        return [...list, ...prev.filter((e) => e.id < oldest)];
      });
      if (firstPage.current) setMore(list.length === pageSize);
      firstPage.current = false;
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [url, pageSize]);

  useEffect(() => {
    firstPage.current = true;
    setEntries(null);
    load();
  }, [load]);

  useLiveRefresh(matches, load);

  async function loadMore() {
    const oldest = entries?.at(-1)?.id;
    if (!oldest) return;
    const list = await api.get<ActivityEntry[]>(`${url}?limit=${pageSize}&before=${oldest}`);
    setEntries((prev) => [...(prev ?? []), ...list]);
    setMore(list.length === pageSize);
  }

  async function undo(entry: ActivityEntry) {
    setBusy(entry.id);
    setError(null);
    try {
      await api.post(`/activity/${entry.id}/revert`);
      await load();
      onReverted?.(entry);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  if (entries === null) return error ? <p className={styles.error}>{error}</p> : <p className={styles.muted}>Loading…</p>;
  if (entries.length === 0) return <p className={styles.muted}>No activity yet.</p>;

  return (
    <div className={styles.feed}>
      {error && <p className={styles.error}>{error}</p>}
      <ol className={styles.list}>
        {entries.map((e) => {
          const canUndo = e.revertible && (e.entity_type !== "comment" || e.actor_id === meId);
          return (
            <li key={e.id} className={styles.entry} data-reverted={e.reverted_by !== null || undefined}>
              <Avatar name={e.actor_name} src={e.actor_avatar_url ?? undefined} size={22} />
              <div className={styles.body}>
                <p className={styles.sentence}>
                  <strong>{e.actor_name}</strong>
                  {e.actor_kind === "bot" && (
                    <span className={styles.bot}>
                      <BotIcon size={9} /> bot
                    </span>
                  )}{" "}
                  {describe(e, { showTarget, onOpenItem, onDescriptionHistory })}
                </p>
                {excerpt(e)}
                <p className={styles.meta}>
                  <time dateTime={e.created_at} title={new Date(e.created_at).toLocaleString()}>
                    {timeAgo(e.created_at)}
                  </time>
                  {e.reverted_by !== null && <span>· undone</span>}
                  {canUndo && (
                    <button type="button" className={styles.undo} disabled={busy === e.id} onClick={() => undo(e)}>
                      {e.action === "deleted" ? "Restore" : "Undo"}
                    </button>
                  )}
                </p>
              </div>
            </li>
          );
        })}
      </ol>
      {more && (
        <button type="button" className={styles.more} onClick={loadMore}>
          Show older activity
        </button>
      )}
    </div>
  );
}

// ---- Sentences --------------------------------------------------------------

const str = (v: unknown) => (v === null || v === undefined || v === "" ? null : String(v));

function stateLabel(v: unknown) {
  return str(v) ? stateInfo(v as never).label : "none";
}

function priorityLabel(v: unknown) {
  return PRIORITIES.find((p) => p.value === v)?.label ?? String(v);
}

function Em({ children }: { children: ReactNode }) {
  return <span className={styles.value}>{children}</span>;
}

/** "a, b and c" */
function list(parts: ReactNode[]): ReactNode {
  return parts.map((p, i) => (
    <span key={i}>
      {i > 0 && (i === parts.length - 1 ? " and " : ", ")}
      {p}
    </span>
  ));
}

/** One phrase per changed field of a work item. */
function itemChanges(changes: Record<string, Change>, it: ReactNode, onDescriptionHistory?: () => void): ReactNode[] {
  const out: ReactNode[] = [];
  for (const [field, c] of Object.entries(changes)) {
    switch (field) {
      case "state":
        out.push(<>moved {it} from <Em>{stateLabel(c.from)}</Em> to <Em>{stateLabel(c.to)}</Em></>);
        break;
      case "priority":
        out.push(<>changed the priority of {it} from <Em>{priorityLabel(c.from)}</Em> to <Em>{priorityLabel(c.to)}</Em></>);
        break;
      case "title":
        out.push(<>renamed {it} to <Em>“{str(c.to)}”</Em></>);
        break;
      case "description":
        out.push(
          <>
            edited the description of {it}
            {onDescriptionHistory && (
              <>
                {" "}
                <button type="button" className={styles.inlineLink} onClick={onDescriptionHistory}>
                  (versions)
                </button>
              </>
            )}
          </>,
        );
        break;
      case "assignee_id":
        out.push(
          str(c.to_label) ? (
            <>assigned {it} to <Em>{str(c.to_label)}</Em></>
          ) : (
            <>unassigned <Em>{str(c.from_label) ?? "someone"}</Em> from {it}</>
          ),
        );
        break;
      case "start_date":
      case "due_date": {
        const what = field === "due_date" ? "due date" : "start date";
        out.push(
          str(c.to) ? (
            <>set the {what} of {it} to <Em>{formatDate(str(c.to))}</Em></>
          ) : (
            <>cleared the {what} of {it}</>
          ),
        );
        break;
      }
      case "parent_id":
        out.push(str(c.to_label) ? <>made {it} a sub-item of <Em>{str(c.to_label)}</Em></> : <>removed the parent of {it}</>);
        break;
      case "epic_id":
        out.push(
          str(c.to_label) ? (
            <>added {it} to the epic <Em>{str(c.to_label)}</Em></>
          ) : (
            <>removed {it} from the epic <Em>{str(c.from_label) ?? ""}</Em></>
          ),
        );
        break;
      case "label_ids": {
        const before = (c.from_label as string[] | null) ?? [];
        const after = (c.to_label as string[] | null) ?? [];
        const added = after.filter((l) => !before.includes(l));
        const removed = before.filter((l) => !after.includes(l));
        if (added.length) out.push(<>labelled {it} <Em>{added.join(", ")}</Em></>);
        if (removed.length) out.push(<>removed the label{removed.length > 1 ? "s" : ""} <Em>{removed.join(", ")}</Em> from {it}</>);
        break;
      }
      default:
        out.push(<>changed the {field.replace(/_id$/, "").replace(/_/g, " ")} of {it}</>);
    }
  }
  return out;
}

function describe(
  e: ActivityEntry,
  opts: { showTarget?: boolean; onOpenItem?: (key: string) => void; onDescriptionHistory?: () => void },
): ReactNode {
  const c = e.changes;
  const label = e.entity_label;
  const key = label.split(" ")[0];
  // The work item an entry is about, as a link in project feeds.
  const item =
    opts.showTarget && e.work_item_id ? (
      opts.onOpenItem && !(e.entity_type === "work_item" && e.action === "deleted") ? (
        <button type="button" className={styles.target} onClick={() => opts.onOpenItem?.(key)}>
          {label}
        </button>
      ) : (
        <Em>{label}</Em>
      )
    ) : null;
  const it = item ?? "it";
  const on = item ? <> on {item}</> : null;
  const to = item ? <> to {item}</> : null;
  const from = item ? <> from {item}</> : null;

  switch (e.entity_type) {
    case "work_item":
      switch (e.action) {
        case "created":
          return <>created {item ?? "this item"}</>;
        case "deleted":
          return <>deleted {item ?? "this item"}</>;
        case "restored":
          return <>restored {item ?? "this item"}</>;
        default: {
          // Name the item once: in the only phrase, or up front when
          // several fields changed together.
          const several = Object.keys(c).length > 1;
          const phrases = list(itemChanges(c, several ? "it" : it, opts.onDescriptionHistory));
          const lead = several && item ? <>updated {item}: </> : null;
          return e.action === "reverted" ? <>undid a change{item && several ? <> to {item}</> : null}: {phrases}</> : <>{lead}{phrases}</>;
        }
      }
    case "comment":
      return {
        created: <>commented{on}</>,
        updated: <>edited a comment{on}</>,
        deleted: <>deleted a comment{on}</>,
        restored: <>restored a comment{on}</>,
        reverted: <>undid a comment edit{on}</>,
      }[e.action] ?? <>changed a comment{on}</>;
    case "relation": {
      const kind = RELATION_LABELS[str(c.kind?.to) as RelationView]?.toLowerCase() ?? "related to";
      const other = <Em>{str(c.item?.to)}</Em>;
      return e.action === "deleted" ? (
        <>removed the relation {item ? <>{item} </> : ""}{kind} {other}</>
      ) : (
        <>marked {it} as {kind} {other}</>
      );
    }
    case "link": {
      const name = <Em>{str(c.title?.to) ?? str(c.url?.to)}</Em>;
      return e.action === "deleted" ? <>removed the link {name}{from}</> : <>added the link {name}{to}</>;
    }
    case "attachment": {
      const name = <Em>{str(c.file_name?.to)}</Em>;
      return e.action === "deleted" ? <>removed the attachment {name}{from}</> : <>attached {name}{to}</>;
    }
    case "reaction":
      return e.action === "deleted" ? <>took back their {str(c.emoji?.to)}{from}</> : <>reacted {str(c.emoji?.to)}{to}</>;
    case "project":
      if (e.action === "created") return <>created the project <Em>{label}</Em></>;
      if (e.action === "deleted") return <>deleted the project <Em>{label}</Em></>;
      return list(
        Object.entries(c).map(([f, v]) =>
          f === "name" ? <>renamed the project to <Em>{str(v.to)}</Em></> :
          f === "identifier" ? <>changed the project ID from <Em>{str(v.from)}</Em> to <Em>{str(v.to)}</Em></> :
          <>changed the project’s {f}</>,
        ),
      );
    case "label":
    case "epic": {
      const what = e.entity_type;
      if (e.action === "created") return <>created the {what} <Em>{label}</Em></>;
      if (e.action === "deleted") return <>deleted the {what} <Em>{label}</Em></>;
      return list(
        Object.entries(c).map(([f, v]) =>
          f === "name" ? <>renamed the {what} <Em>{str(v.from)}</Em> to <Em>{str(v.to)}</Em></> : <>changed the {f} of the {what} <Em>{label}</Em></>,
        ),
      );
    }
    case "member":
      if (e.action === "created") return <>added <Em>{label}</Em> to the workspace as {str(c.role?.to)}</>;
      if (e.action === "deleted") return <>removed <Em>{label}</Em> from the workspace</>;
      return <>made <Em>{label}</Em> {str(c.role?.to) === "admin" ? "an admin" : "a member"}</>;
    case "bot":
      if (e.action === "created") return <>created the bot <Em>{label}</Em></>;
      if (e.action === "deleted") return <>deleted the bot <Em>{label}</Em></>;
      return c.name ? <>renamed the bot <Em>{str(c.name.from)}</Em> to <Em>{str(c.name.to)}</Em></> : <>updated the bot <Em>{label}</Em></>;
    case "bot_token":
      return e.action === "revoked" ? <>revoked the token <Em>{label}</Em></> : <>created the token <Em>{label}</Em></>;
    case "workspace":
      return e.action === "created" ? <>created the workspace <Em>{label}</Em></> : <>renamed the workspace to <Em>{label}</Em></>;
    default:
      return <>{e.action} {e.entity_type.replace(/_/g, " ")} <Em>{label}</Em></>;
  }
}

/** A short quote for comments and comment edits. */
function excerpt(e: ActivityEntry): ReactNode {
  if (e.entity_type !== "comment" || e.action === "deleted") return null;
  const text = str(e.changes.body?.to);
  return text ? <blockquote className={styles.quote}>{text}</blockquote> : null;
}
