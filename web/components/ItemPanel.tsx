"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Avatar } from "gui-mellow/Avatar";
import { Button } from "gui-mellow/Button";
import { ConfirmDialog } from "gui-mellow/Dialog";
import { Drawer } from "gui-mellow/Drawer";
import { SelectField, TextField } from "gui-mellow/Field";
import { toneColor } from "gui-mellow/utils";
import {
  ApiError,
  PRIORITIES,
  RELATION_KINDS,
  RELATION_LABELS,
  STATES,
  api,
  errorMessage,
  formatBytes,
  stateInfo,
  timeAgo,
  type Comment,
  type Epic,
  type Label,
  type Member,
  type ReactionGroup,
  type RelationKind,
  type WorkItem,
  type WorkItemDetail,
} from "@/lib/api";
import { useLiveRefresh } from "@/lib/live";
import { useSession } from "@/lib/session";
import { ActivityFeed } from "./ActivityFeed";
import { DescriptionHistory } from "./DescriptionHistory";
import { BotIcon, PriorityIcon, StateIcon } from "./Icons";
import { RichText, RichTextEditor, type RichTextHandle } from "./RichTextEditor";
import styles from "./ItemPanel.module.scss";

const QUICK_REACTIONS = ["👍", "🎉", "❤️", "🚀", "👀", "😄", "🤔", "🔥"];

interface Props {
  itemKey: string | null;
  projectItems: WorkItem[];
  labels: Label[];
  epics: Epic[];
  members: Member[];
  onClose: () => void;
  onOpen: (key: string) => void;
  /** The board's copy of the item changed (or it was deleted, when null). */
  onChanged: (id: string, item: WorkItem | null) => void;
}

function Section({ title, children, action }: { title: string; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <section className={styles.section}>
      <header className={styles.sectionHeader}>
        <h3>{title}</h3>
        {action}
      </header>
      {children}
    </section>
  );
}

export function ItemPanel({ itemKey, projectItems, labels, epics, members, onClose, onOpen, onChanged }: Props) {
  const { me } = useSession();
  const [item, setItem] = useState<WorkItemDetail | null>(null);
  const [comments, setComments] = useState<Comment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const titleRef = useRef<HTMLTextAreaElement>(null);

  const load = useCallback(async (key: string) => {
    setError(null);
    try {
      const detail = await api.get<WorkItemDetail>(`/work-items/${encodeURIComponent(key)}`);
      setItem(detail);
      setTitle(detail.title);
      setComments(await api.get<Comment[]>(`/work-items/${detail.id}/comments`));
    } catch (e) {
      setError(errorMessage(e));
      setItem(null);
    }
  }, []);

  useEffect(() => {
    if (itemKey) load(itemKey);
    else setItem(null);
  }, [itemKey, load]);

  // Someone else (or another tab, or a bot) changed this item: re-fetch,
  // leaving alone anything being typed. The description editor already
  // ignores outside changes while it has focus.
  const itemId = item?.id;
  const onChangedRef = useRef(onChanged);
  onChangedRef.current = onChanged;
  const refresh = useCallback(async () => {
    if (!itemId) return;
    try {
      const detail = await api.get<WorkItemDetail>(`/work-items/${itemId}`);
      setItem(detail);
      onChangedRef.current(detail.id, detail);
      if (document.activeElement !== titleRef.current) setTitle(detail.title);
      setComments(await api.get<Comment[]>(`/work-items/${itemId}/comments`));
    } catch (e) {
      setItem(null);
      setError(e instanceof ApiError && e.status === 404 ? "This item has been deleted." : errorMessage(e));
    }
  }, [itemId]);
  useLiveRefresh(
    (e) => !!itemId && (e.work_item_id === itemId || e.related_item_id === itemId || e.entity_id === itemId),
    refresh,
  );

  /** Applies a server response and tells the board. */
  const apply = useCallback(
    (detail: WorkItemDetail) => {
      setItem(detail);
      onChanged(detail.id, detail);
    },
    [onChanged],
  );

  const patch = useCallback(
    async (changes: Record<string, unknown>) => {
      if (!item) return;
      setError(null);
      try {
        apply(await api.patch<WorkItemDetail>(`/work-items/${item.id}`, changes));
      } catch (e) {
        setError(errorMessage(e));
      }
    },
    [item, apply],
  );

  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const open = itemKey !== null;
  const accent = item ? toneColor(stateInfo(item.state).tone) : undefined;

  return (
    <Drawer
      open={open}
      onClose={onClose}
      title={item ? item.key : itemKey ?? ""}
      subtitle={item ? `Created by ${item.created_by_name ?? "someone"} · ${timeAgo(item.created_at)}` : undefined}
      accent={accent}
      className={styles.drawer}
    >
      {error && <p className={styles.error}>{error}</p>}
      {!item && !error && open && <div className="methods-loading">Loading…</div>}
      {item && (
        <div className={styles.panel}>
          <textarea
            ref={titleRef}
            className={styles.title}
            aria-label="Title"
            value={title}
            rows={1}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                e.currentTarget.blur();
              }
            }}
            onBlur={() => title.trim() && title !== item.title && patch({ title })}
          />

          <Properties item={item} epics={epics} members={members} projectItems={projectItems} onPatch={patch} />

          <LabelPicker item={item} labels={labels} onPatch={patch} />

          <Section
            title="Description"
            action={
              <button type="button" className={styles.sectionLink} onClick={() => setHistoryOpen(true)}>
                History
              </button>
            }
          >
            <RichTextEditor
              key={item.id}
              label="Description"
              value={item.description}
              placeholder="Add a description…"
              onBlur={(doc) => patch({ description: doc })}
            />
          </Section>

          <Reactions item={item} onChange={(reactions) => setItem({ ...item, reactions })} />

          <SubItems item={item} onOpen={onOpen} onCreated={() => load(item.key)} />

          <Relations item={item} projectItems={projectItems} onOpen={onOpen} onChange={apply} run={run} />

          <Links item={item} onChange={(links) => setItem({ ...item, links })} run={run} />

          <Attachments item={item} onChange={apply} run={run} />

          <Comments
            item={item}
            comments={comments}
            meId={me.id}
            onChange={(next) => {
              setComments(next);
              onChanged(item.id, { ...item, comment_count: next.length });
            }}
            run={run}
          />

          <Section title="Activity">
            <ActivityFeed
              url={`/work-items/${item.id}/activity`}
              matches={(e) => e.id !== null && (e.work_item_id === item.id || e.related_item_id === item.id)}
              meId={me.id}
              pageSize={15}
              onDescriptionHistory={() => setHistoryOpen(true)}
              onReverted={refresh}
            />
          </Section>

          <DescriptionHistory
            itemId={item.id}
            open={historyOpen}
            onClose={() => setHistoryOpen(false)}
            onRestore={async (v) => {
              apply(await api.patch<WorkItemDetail>(`/work-items/${item.id}`, { description: v.description }));
            }}
          />

          <footer className={styles.footer}>
            <span>Updated {timeAgo(item.updated_at)}</span>
            <Button variant="danger" size="sm" onClick={() => setConfirmDelete(true)}>
              Delete item
            </Button>
          </footer>

          <ConfirmDialog
            open={confirmDelete}
            title={`Delete ${item.key}?`}
            destructive
            confirmLabel="Delete"
            onCancel={() => setConfirmDelete(false)}
            onConfirm={() =>
              run(async () => {
                await api.del(`/work-items/${item.id}`);
                setConfirmDelete(false);
                onChanged(item.id, null);
                onClose();
              })
            }
          >
            The item and its comments move to the trash. You can restore it from the project’s activity.
          </ConfirmDialog>
        </div>
      )}
    </Drawer>
  );
}

// ---- Properties ------------------------------------------------------------

function Properties({
  item,
  epics,
  members,
  projectItems,
  onPatch,
}: {
  item: WorkItemDetail;
  epics: Epic[];
  members: Member[];
  projectItems: WorkItem[];
  onPatch: (c: Record<string, unknown>) => void;
}) {
  const humans = members.filter((m) => m.kind === "human");
  const bots = members.filter((m) => m.kind === "bot");
  const nullable = (v: string) => (v === "" ? null : v);
  return (
    <div className={styles.props}>
      <div className={styles.withIcon}>
        <StateIcon state={item.state} />
        <SelectField
          label="State"
          value={item.state}
          options={STATES.map((s) => ({ value: s.value, label: s.label }))}
          onChange={(e) => onPatch({ state: e.target.value })}
        />
      </div>
      <div className={styles.withIcon}>
        <PriorityIcon priority={item.priority} />
        <SelectField
          label="Priority"
          value={item.priority}
          options={PRIORITIES}
          onChange={(e) => onPatch({ priority: e.target.value })}
        />
      </div>
      <SelectField
        label="Assignee"
        value={item.assignee_id ?? ""}
        onChange={(e) => onPatch({ assignee_id: nullable(e.target.value) })}
      >
        <option value="">Unassigned</option>
        <optgroup label="People">
          {humans.map((m) => (
            <option key={m.id} value={m.id}>
              {m.display_name}
            </option>
          ))}
        </optgroup>
        {bots.length > 0 && (
          <optgroup label="Bots">
            {bots.map((m) => (
              <option key={m.id} value={m.id}>
                🤖 {m.display_name}
              </option>
            ))}
          </optgroup>
        )}
      </SelectField>
      <SelectField label="Epic" value={item.epic_id ?? ""} onChange={(e) => onPatch({ epic_id: nullable(e.target.value) })}>
        <option value="">No epic</option>
        {epics.map((e) => (
          <option key={e.id} value={e.id}>
            {e.name}
          </option>
        ))}
      </SelectField>
      <TextField
        label="Start date"
        type="date"
        value={item.start_date ?? ""}
        onChange={(e) => onPatch({ start_date: nullable(e.target.value) })}
      />
      <TextField
        label="Due date"
        type="date"
        value={item.due_date ?? ""}
        min={item.start_date ?? undefined}
        onChange={(e) => onPatch({ due_date: nullable(e.target.value) })}
      />
      <SelectField
        label="Parent"
        className={styles.full}
        value={item.parent_id ?? ""}
        onChange={(e) => onPatch({ parent_id: nullable(e.target.value) })}
      >
        <option value="">No parent</option>
        {projectItems
          .filter((p) => p.id !== item.id)
          .sort((a, b) => a.seq - b.seq)
          .map((p) => (
            <option key={p.id} value={p.id}>
              {p.key} · {p.title}
            </option>
          ))}
      </SelectField>
    </div>
  );
}

function LabelPicker({
  item,
  labels,
  onPatch,
}: {
  item: WorkItemDetail;
  labels: Label[];
  onPatch: (c: Record<string, unknown>) => void;
}) {
  if (labels.length === 0) return null;
  const selected = new Set(item.labels.map((l) => l.id));
  return (
    <div className={styles.labels} role="group" aria-label="Labels">
      {labels.map((l) => {
        const on = selected.has(l.id);
        return (
          <button
            key={l.id}
            type="button"
            aria-pressed={on}
            className={`${styles.labelChip} ${on ? styles.on : ""}`}
            style={{ "--c": l.color } as React.CSSProperties}
            onClick={() => {
              const next = new Set(selected);
              if (on) next.delete(l.id);
              else next.add(l.id);
              onPatch({ label_ids: [...next] });
            }}
          >
            {l.name}
          </button>
        );
      })}
    </div>
  );
}

// ---- Reactions -------------------------------------------------------------

function Reactions({ item, onChange }: { item: WorkItemDetail; onChange: (r: ReactionGroup[]) => void }) {
  const [picking, setPicking] = useState(false);
  const toggle = async (emoji: string) => {
    setPicking(false);
    onChange(await api.post<ReactionGroup[]>(`/work-items/${item.id}/reactions`, { emoji }));
  };
  return (
    <div className={styles.reactions}>
      {item.reactions.map((r) => (
        <button
          key={r.emoji}
          type="button"
          className={`${styles.reaction} ${r.reacted ? styles.on : ""}`}
          title={r.users.join(", ")}
          aria-pressed={r.reacted}
          onClick={() => toggle(r.emoji)}
        >
          {r.emoji} <span>{r.count}</span>
        </button>
      ))}
      <span className={styles.reactionPicker}>
        <button type="button" className={styles.reaction} onClick={() => setPicking((p) => !p)} aria-label="Add reaction">
          ☺︎+
        </button>
        {picking && (
          <span className={styles.reactionMenu}>
            {QUICK_REACTIONS.map((e) => (
              <button key={e} type="button" onClick={() => toggle(e)} aria-label={`React with ${e}`}>
                {e}
              </button>
            ))}
          </span>
        )}
      </span>
    </div>
  );
}

// ---- Sub-items, relations, links, attachments -----------------------------

function ItemRow({
  itemKey,
  title,
  state,
  prefix,
  onOpen,
  onRemove,
}: {
  itemKey: string;
  title: string;
  state: WorkItem["state"];
  prefix?: string;
  onOpen: () => void;
  onRemove?: () => void;
}) {
  return (
    <li className={styles.row}>
      <button type="button" className={styles.rowMain} onClick={onOpen}>
        <StateIcon state={state} />
        {prefix && <span className={styles.rowPrefix}>{prefix}</span>}
        <span className={styles.rowKey}>{itemKey}</span>
        <span className={styles.rowTitle}>{title}</span>
      </button>
      {onRemove && (
        <button type="button" className={styles.remove} onClick={onRemove} aria-label="Remove">
          ×
        </button>
      )}
    </li>
  );
}

function SubItems({ item, onOpen, onCreated }: { item: WorkItemDetail; onOpen: (k: string) => void; onCreated: () => void }) {
  const [title, setTitle] = useState("");
  const [adding, setAdding] = useState(false);
  async function add() {
    if (!title.trim()) return;
    await api.post(`/projects/${item.project_id}/work-items`, { title, parent_id: item.id, state: "todo" });
    setTitle("");
    onCreated();
  }
  return (
    <Section
      title={`Sub-items${item.children.length ? ` · ${item.children.length}` : ""}`}
      action={
        <Button size="sm" variant="ghost" onClick={() => setAdding((a) => !a)}>
          + Add
        </Button>
      }
    >
      {item.parent && (
        <ul className={styles.list}>
          <ItemRow itemKey={item.parent.key} title={item.parent.title} state={item.parent.state} prefix="Parent" onOpen={() => onOpen(item.parent!.key)} />
        </ul>
      )}
      {item.children.length > 0 && (
        <ul className={styles.list}>
          {item.children.map((c) => (
            <ItemRow key={c.id} itemKey={c.key} title={c.title} state={c.state} onOpen={() => onOpen(c.key)} />
          ))}
        </ul>
      )}
      {adding && (
        <form
          className={styles.inline}
          onSubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <TextField label="Sub-item title" hideLabel placeholder="Sub-item title" value={title} autoFocus onChange={(e) => setTitle(e.target.value)} />
          <Button type="submit" size="sm" disabled={!title.trim()}>
            Add
          </Button>
        </form>
      )}
    </Section>
  );
}

function Relations({
  item,
  projectItems,
  onOpen,
  onChange,
  run,
}: {
  item: WorkItemDetail;
  projectItems: WorkItem[];
  onOpen: (k: string) => void;
  onChange: (d: WorkItemDetail) => void;
  run: (fn: () => Promise<unknown>) => void;
}) {
  const [kind, setKind] = useState<RelationKind>("relates_to");
  const [target, setTarget] = useState("");
  const [adding, setAdding] = useState(false);
  return (
    <Section
      title="Relations"
      action={
        <Button size="sm" variant="ghost" onClick={() => setAdding((a) => !a)}>
          + Add
        </Button>
      }
    >
      {item.relations.length > 0 && (
        <ul className={styles.list}>
          {item.relations.map((r) => (
            <ItemRow
              key={r.id}
              itemKey={r.item_key}
              title={r.item_title}
              state={r.item_state}
              prefix={RELATION_LABELS[r.kind]}
              onOpen={() => onOpen(r.item_key)}
              onRemove={() =>
                run(async () => {
                  await api.del(`/relations/${r.id}`);
                  onChange({ ...item, relations: item.relations.filter((x) => x.id !== r.id) });
                })
              }
            />
          ))}
        </ul>
      )}
      {item.relations.length === 0 && !adding && <p className={styles.muted}>No related items.</p>}
      {adding && (
        <form
          className={styles.inline}
          onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              onChange(await api.post<WorkItemDetail>(`/work-items/${item.id}/relations`, { kind, target_id: target }));
              setTarget("");
              setAdding(false);
            });
          }}
        >
          <SelectField label="Kind" hideLabel value={kind} options={RELATION_KINDS} onChange={(e) => setKind(e.target.value as RelationKind)} />
          <SelectField label="Item" hideLabel value={target} onChange={(e) => setTarget(e.target.value)} className={styles.grow}>
            <option value="">Choose an item…</option>
            {projectItems
              .filter((p) => p.id !== item.id)
              .sort((a, b) => a.seq - b.seq)
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.key} · {p.title}
                </option>
              ))}
          </SelectField>
          <Button type="submit" size="sm" disabled={!target}>
            Add
          </Button>
        </form>
      )}
    </Section>
  );
}

function Links({
  item,
  onChange,
  run,
}: {
  item: WorkItemDetail;
  onChange: (l: WorkItemDetail["links"]) => void;
  run: (fn: () => Promise<unknown>) => void;
}) {
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [adding, setAdding] = useState(false);
  return (
    <Section
      title="Links"
      action={
        <Button size="sm" variant="ghost" onClick={() => setAdding((a) => !a)}>
          + Add
        </Button>
      }
    >
      {item.links.length > 0 && (
        <ul className={styles.list}>
          {item.links.map((l) => (
            <li key={l.id} className={styles.row}>
              <a href={l.url} target="_blank" rel="noreferrer noopener" className={styles.rowMain}>
                <span aria-hidden>🔗</span>
                <span className={styles.rowTitle}>{l.title || l.url}</span>
                {l.title && <span className={styles.rowHost}>{safeHost(l.url)}</span>}
              </a>
              <button
                type="button"
                className={styles.remove}
                aria-label="Remove link"
                onClick={() =>
                  run(async () => {
                    await api.del(`/links/${l.id}`);
                    onChange(item.links.filter((x) => x.id !== l.id));
                  })
                }
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      {item.links.length === 0 && !adding && <p className={styles.muted}>No links.</p>}
      {adding && (
        <form
          className={styles.inline}
          onSubmit={(e) => {
            e.preventDefault();
            run(async () => {
              const href = /^[a-z]+:/i.test(url.trim()) ? url.trim() : `https://${url.trim()}`;
              const link = await api.post<WorkItemDetail["links"][number]>(`/work-items/${item.id}/links`, { url: href, title });
              onChange([...item.links, link]);
              setUrl("");
              setTitle("");
              setAdding(false);
            });
          }}
        >
          <TextField label="URL" hideLabel placeholder="https://…" value={url} autoFocus className={styles.grow} onChange={(e) => setUrl(e.target.value)} />
          <TextField label="Title" hideLabel placeholder="Title (optional)" value={title} onChange={(e) => setTitle(e.target.value)} />
          <Button type="submit" size="sm" disabled={!url.trim()}>
            Add
          </Button>
        </form>
      )}
    </Section>
  );
}

function safeHost(url: string) {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

function Attachments({
  item,
  onChange,
  run,
}: {
  item: WorkItemDetail;
  onChange: (d: WorkItemDetail) => void;
  run: (fn: () => Promise<unknown>) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  async function upload(files: FileList | null) {
    if (!files?.length) return;
    setBusy(true);
    await run(async () => {
      let latest = item;
      for (const file of Array.from(files)) {
        const form = new FormData();
        form.append("file", file, file.name);
        const res = await fetch(`/api/work-items/${item.id}/attachments`, { method: "POST", body: form });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error ?? "Upload failed");
        latest = data;
      }
      onChange(latest);
    });
    setBusy(false);
    if (input.current) input.current.value = "";
  }
  return (
    <Section
      title="Attachments"
      action={
        <Button size="sm" variant="ghost" loading={busy} onClick={() => input.current?.click()}>
          + Upload
        </Button>
      }
    >
      <div
        className={styles.dropzone}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          upload(e.dataTransfer.files);
        }}
      >
        {item.attachments.length === 0 ? (
          <p className={styles.muted}>Drop files here, or upload designs, documents and screenshots.</p>
        ) : (
          <ul className={styles.attachments}>
            {item.attachments.map((a) => (
              <li key={a.id} className={styles.attachment}>
                <a href={a.url} target="_blank" rel="noreferrer" className={styles.thumb}>
                  {a.content_type.startsWith("image/") && !a.content_type.includes("svg") ? (
                    <img src={a.url} alt="" loading="lazy" />
                  ) : (
                    <span>{a.file_name.split(".").pop()?.slice(0, 4).toUpperCase() || "FILE"}</span>
                  )}
                </a>
                <span className={styles.attachmentInfo}>
                  <a href={a.url} target="_blank" rel="noreferrer">
                    {a.file_name}
                  </a>
                  <span>
                    {formatBytes(a.size_bytes)} · {timeAgo(a.created_at)}
                  </span>
                </span>
                <button
                  type="button"
                  className={styles.remove}
                  aria-label={`Remove ${a.file_name}`}
                  onClick={() =>
                    run(async () => {
                      await api.del(`/attachments/${a.id}`);
                      onChange({ ...item, attachments: item.attachments.filter((x) => x.id !== a.id), attachment_count: item.attachment_count - 1 });
                    })
                  }
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <input ref={input} type="file" multiple hidden onChange={(e) => upload(e.target.files)} />
    </Section>
  );
}

// ---- Comments --------------------------------------------------------------

function Comments({
  item,
  comments,
  meId,
  onChange,
  run,
}: {
  item: WorkItemDetail;
  comments: Comment[];
  meId: string;
  onChange: (c: Comment[]) => void;
  run: (fn: () => Promise<unknown>) => void;
}) {
  const editor = useRef<RichTextHandle>(null);
  const [hasDraft, setHasDraft] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const editEditor = useRef<RichTextHandle>(null);

  async function submit() {
    const body = editor.current?.getDoc();
    if (!body) return;
    setBusy(true);
    await run(async () => {
      const c = await api.post<Comment>(`/work-items/${item.id}/comments`, { body });
      onChange([...comments, c]);
      editor.current?.clear();
    });
    setBusy(false);
  }

  return (
    <Section title={`Comments${comments.length ? ` · ${comments.length}` : ""}`}>
      <ol className={styles.comments}>
        {comments.map((c) => (
          <li key={c.id} className={styles.comment}>
            <Avatar name={c.author_name ?? "Deleted user"} src={c.author_avatar_url ?? undefined} size={28} />
            <div className={styles.commentBody}>
              <p className={styles.commentMeta}>
                <strong>{c.author_name ?? "Deleted user"}</strong>
                {c.author_kind === "bot" && (
                  <span className={styles.botTag}>
                    <BotIcon size={10} /> bot
                  </span>
                )}
                <span>{timeAgo(c.created_at)}</span>
                {c.updated_at !== c.created_at && <span>(edited)</span>}
                {c.author_id === meId && editing !== c.id && (
                  <span className={styles.commentActions}>
                    <button type="button" onClick={() => setEditing(c.id)}>
                      Edit
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        run(async () => {
                          await api.del(`/comments/${c.id}`);
                          onChange(comments.filter((x) => x.id !== c.id));
                        })
                      }
                    >
                      Delete
                    </button>
                  </span>
                )}
              </p>
              {editing === c.id ? (
                <div className={styles.editComment}>
                  <RichTextEditor ref={editEditor} label="Edit comment" value={c.body} minHeight="3rem" />
                  <div className={styles.inline}>
                    <Button
                      size="sm"
                      onClick={() =>
                        run(async () => {
                          const updated = await api.patch<Comment>(`/comments/${c.id}`, {
                            body: editEditor.current?.getDoc() ?? null,
                          });
                          onChange(comments.map((x) => (x.id === c.id ? updated : x)));
                          setEditing(null);
                        })
                      }
                    >
                      Save
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>
                      Cancel
                    </Button>
                  </div>
                </div>
              ) : (
                <RichText doc={c.body} className={styles.prose} />
              )}
            </div>
          </li>
        ))}
      </ol>
      <div className={styles.newComment}>
        <RichTextEditor
          ref={editor}
          label="New comment"
          placeholder="Write a comment… (⌘↵ to send)"
          minHeight="3.5rem"
          onChange={(doc) => setHasDraft(!!doc)}
          onSubmit={submit}
        />
        <Button size="sm" onClick={submit} loading={busy} disabled={!hasDraft}>
          Comment
        </Button>
      </div>
    </Section>
  );
}
