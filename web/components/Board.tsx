"use client";

import { memo, useCallback, useLayoutEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { Avatar } from "gui-mellow/Avatar";
import { toneColor } from "gui-mellow/utils";
import { STATES, formatDate, type Epic, type Label, type State, type WorkItem } from "@/lib/api";
import { BotIcon, PriorityIcon, StateIcon } from "./Icons";
import styles from "./Board.module.scss";

interface Props {
  items: WorkItem[];
  labels: Map<string, Label>;
  epics: Map<string, Epic>;
  selectedKey: string | null;
  onOpen: (item: WorkItem) => void;
  onMove: (item: WorkItem, state: State, sortOrder: number) => void;
  onAdd: (state: State) => void;
  hiddenStates?: State[];
}

function isOverdue(item: WorkItem) {
  if (!item.due_date || item.state === "done" || item.state === "cancelled") return false;
  return new Date(`${item.due_date}T23:59:59`) < new Date();
}

/**
 * Memoised: every callback it gets is stable, so a card re-renders only when
 * its own item, epic, labels or selection change, not on each drag-over or
 * live refresh of the board.
 */
const ItemCard = memo(function ItemCard({
  item,
  labels,
  epic,
  selected,
  onOpen,
  onDragStart,
  onDragEnd,
}: {
  item: WorkItem;
  labels: Map<string, Label>;
  epic: Epic | undefined;
  selected: boolean;
  onOpen: (item: WorkItem) => void;
  onDragStart: (item: WorkItem, e: DragEvent) => void;
  onDragEnd: () => void;
}) {
  return (
    <button
      type="button"
      className={`${styles.card} ${selected ? styles.selected : ""}`}
      draggable
      onDragStart={(e) => onDragStart(item, e)}
      onDragEnd={onDragEnd}
      onClick={() => onOpen(item)}
      data-item={item.id}
      data-glow
    >
      <span className={styles.cardTop}>
        <span className={styles.key}>{item.key}</span>
        <PriorityIcon priority={item.priority} />
      </span>
      <span className={styles.title}>{item.title}</span>
      {(item.label_ids.length > 0 || epic) && (
        <span className={styles.tags}>
          {epic && (
            <span className={styles.epic} style={{ "--c": epic.color } as React.CSSProperties}>
              ◆ {epic.name}
            </span>
          )}
          {item.label_ids.map((id) => {
            const l = labels.get(id);
            return l ? (
              <span key={id} className={styles.label} style={{ "--c": l.color } as React.CSSProperties}>
                {l.name}
              </span>
            ) : null;
          })}
        </span>
      )}
      <span className={styles.cardBottom}>
        <span className={styles.meta}>
          {item.due_date && (
            <span className={isOverdue(item) ? styles.overdue : undefined} title="Due date">
              ⏰ {formatDate(item.due_date)}
            </span>
          )}
          {item.child_count > 0 && <span title="Sub-items">↳ {item.child_count}</span>}
          {item.comment_count > 0 && <span title="Comments">💬 {item.comment_count}</span>}
          {item.attachment_count > 0 && <span title="Attachments">📎 {item.attachment_count}</span>}
        </span>
        {item.assignee_name && (
          <span className={styles.assignee} title={`Assigned to ${item.assignee_name}`}>
            <Avatar name={item.assignee_name} src={item.assignee_avatar_url ?? undefined} size={22} />
            {item.assignee_kind === "bot" && (
              <span className={styles.botBadge}>
                <BotIcon size={9} />
              </span>
            )}
          </span>
        )}
      </span>
    </button>
  );
});

/** Where a new item lands between neighbours. */
function orderBetween(before: WorkItem | undefined, after: WorkItem | undefined) {
  if (before && after) return (before.sort_order + after.sort_order) / 2;
  if (before) return before.sort_order + 1024;
  if (after) return after.sort_order - 1024;
  return 1024;
}

export function Board({ items, labels, epics, selectedKey, onOpen, onMove, onAdd, hiddenStates = [] }: Props) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [drop, setDrop] = useState<{ state: State; index: number } | null>(null);

  // The page passes a new onOpen on each render; cards get a stable wrapper.
  const onOpenRef = useRef(onOpen);
  useLayoutEffect(() => {
    onOpenRef.current = onOpen;
  });
  const openCard = useCallback((item: WorkItem) => onOpenRef.current(item), []);
  const startDrag = useCallback((item: WorkItem, e: DragEvent) => {
    e.dataTransfer.setData("text/plain", item.key);
    e.dataTransfer.effectAllowed = "move";
    // Let the browser snapshot the card before we hide it.
    requestAnimationFrame(() => setDragId(item.id));
  }, []);
  const endDrag = useCallback(() => {
    setDragId(null);
    setDrop(null);
  }, []);

  const columns = useMemo(() => {
    const by = new Map<State, WorkItem[]>(STATES.map((s) => [s.value, []]));
    for (const item of items) by.get(item.state)?.push(item);
    for (const list of by.values()) list.sort((a, b) => a.sort_order - b.sort_order || a.seq - b.seq);
    return by;
  }, [items]);

  function indexAt(e: DragEvent, state: State) {
    const list = e.currentTarget.querySelectorAll<HTMLElement>("[data-item]");
    let index = 0;
    for (const el of list) {
      if (el.dataset.item === dragId) continue;
      const r = el.getBoundingClientRect();
      if (e.clientY > r.top + r.height / 2) index++;
    }
    return { state, index };
  }

  function handleDrop(state: State) {
    const item = items.find((i) => i.id === dragId);
    const target = drop;
    setDragId(null);
    setDrop(null);
    if (!item || !target) return;
    const list = (columns.get(state) ?? []).filter((i) => i.id !== item.id);
    const order = orderBetween(list[target.index - 1], list[target.index]);
    if (item.state === state && Math.abs(order - item.sort_order) < 1e-9) return;
    onMove(item, state, order);
  }

  return (
    <div className={styles.board}>
      {STATES.filter((s) => !hiddenStates.includes(s.value)).map((s) => {
        const list = columns.get(s.value) ?? [];
        const visible = list.filter((i) => i.id !== dragId);
        return (
          <section
            key={s.value}
            className={`${styles.column} ${drop?.state === s.value ? styles.over : ""}`}
            style={{ "--c": toneColor(s.tone) } as React.CSSProperties}
            aria-label={s.label}
            onDragOver={(e) => {
              if (!dragId) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
              const next = indexAt(e, s.value);
              if (next.state !== drop?.state || next.index !== drop?.index) setDrop(next);
            }}
            onDragLeave={(e) => {
              if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDrop(null);
            }}
            onDrop={(e) => {
              e.preventDefault();
              handleDrop(s.value);
            }}
          >
            <header className={styles.columnHeader}>
              <StateIcon state={s.value} />
              <h2>{s.label}</h2>
              <span className={styles.count}>{list.length}</span>
              <button type="button" className={styles.add} onClick={() => onAdd(s.value)} aria-label={`Add to ${s.label}`}>
                +
              </button>
            </header>
            <div className={styles.cards}>
              {visible.map((item, index) => (
                  <div key={item.id} className={styles.slot}>
                    {drop?.state === s.value && drop.index === index && <div className={styles.indicator} />}
                    <ItemCard
                      item={item}
                      labels={labels}
                      epic={item.epic_id ? epics.get(item.epic_id) : undefined}
                      selected={item.key === selectedKey}
                      onOpen={openCard}
                      onDragStart={startDrag}
                      onDragEnd={endDrag}
                    />
                  </div>
                ))}
              {drop?.state === s.value && drop.index >= visible.length && (
                <div className={styles.indicator} />
              )}
              {list.length === 0 && !drop && <p className={styles.empty}>No items</p>}
            </div>
          </section>
        );
      })}
    </div>
  );
}
