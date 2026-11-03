"use client";

import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import styles from "./StackedBar.module.scss";

export type BarSegment = { key: string; label: string; value: number; color: string };

const OPEN_DELAY = 120; // ms of hover before the first popover appears
const CLOSE_GRACE = 140; // ms before closing, so brief exits don't flicker
const GAP = 8; // px between the bar and the popover
const MARGIN = 8; // px kept clear of the viewport edges

/**
 * A continuous bar split into segments, with a frosted popover naming the
 * segment under the pointer ("In Progress · 3 · 27%").
 *
 * Interim stand-in for gui-mellow's stacked Meter/Stat (GM-11) and Popover
 * (GM-12). The popover uses the native Popover API, so it renders in the top
 * layer and isn't clipped by the card's `overflow: hidden`.
 *
 * The bar usually lives inside a link (a project card), where focusable
 * children aren't allowed. So keyboard and screen-reader users get the full
 * breakdown as text instead, and on touch the first tap on the bar shows the
 * popover rather than following the link.
 */
export function StackedBar({ segments, className }: { segments: BarSegment[]; className?: string }) {
  const visible = segments.filter((s) => s.value > 0);
  const total = visible.reduce((a, s) => a + s.value, 0);

  const barRef = useRef<HTMLDivElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const segRefs = useRef(new Map<string, HTMLSpanElement>());
  const openTimer = useRef<number>(undefined);
  const closeTimer = useRef<number>(undefined);
  const lastPointer = useRef<string>("mouse");
  const [active, setActive] = useState<string | null>(null);

  const clearTimers = () => {
    window.clearTimeout(openTimer.current);
    window.clearTimeout(closeTimer.current);
  };

  const close = useCallback(() => {
    clearTimers();
    setActive(null);
  }, []);

  // Place the popover above the active segment, flipping below near the top
  // of the viewport and clamping to its sides.
  const place = useCallback((key: string) => {
    const pop = popRef.current;
    const seg = segRefs.current.get(key);
    if (!pop || !seg) return;
    const r = seg.getBoundingClientRect();
    const p = pop.getBoundingClientRect();
    const centre = r.left + r.width / 2;
    const left = Math.min(Math.max(centre - p.width / 2, MARGIN), window.innerWidth - p.width - MARGIN);
    const above = r.top - GAP - p.height;
    const below = above < MARGIN;
    pop.style.left = `${left}px`;
    pop.style.top = `${below ? r.bottom + GAP : above}px`;
    pop.dataset.side = below ? "bottom" : "top";
    // Point the arrow at the segment's centre even when the box was clamped.
    pop.style.setProperty("--arrow-x", `${Math.min(Math.max(centre - left, 12), p.width - 12)}px`);
  }, []);

  useEffect(() => {
    const pop = popRef.current;
    if (!pop) return;
    if (!active) {
      if (pop.matches(":popover-open")) pop.hidePopover();
      return;
    }
    if (!pop.matches(":popover-open")) pop.showPopover();
    place(active);
  }, [active, place]);

  // While open, follow the bar through scrolls and resizes, and close on a
  // tap or click anywhere else.
  useEffect(() => {
    if (!active) return;
    let frame = 0;
    const follow = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => place(active));
    };
    const onPointerDown = (e: PointerEvent) => {
      if (!barRef.current?.contains(e.target as Node)) close();
    };
    window.addEventListener("scroll", follow, { passive: true, capture: true });
    window.addEventListener("resize", follow);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", follow, { capture: true });
      window.removeEventListener("resize", follow);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [active, close, place]);

  useEffect(() => clearTimers, []);

  // Which segment is under this x position. The whole bar's height (including
  // its padded hit area) counts, so a 6px bar is still easy to hover.
  const segmentAt = (x: number) => {
    for (const s of visible) {
      const r = segRefs.current.get(s.key)?.getBoundingClientRect();
      if (r && x >= r.left && x < r.right) return s.key;
    }
    // Past either end (the padded hit area is wider than the rounded caps).
    const first = segRefs.current.get(visible[0]?.key ?? "")?.getBoundingClientRect();
    return first && x < first.left ? (visible[0]?.key ?? null) : (visible.at(-1)?.key ?? null);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    lastPointer.current = e.pointerType;
    if (e.pointerType !== "mouse") return;
    const key = segmentAt(e.clientX);
    window.clearTimeout(closeTimer.current);
    if (active) {
      // Already open: follow the pointer from segment to segment instantly.
      if (key !== active) setActive(key);
      return;
    }
    window.clearTimeout(openTimer.current);
    openTimer.current = window.setTimeout(() => setActive(key), OPEN_DELAY);
  };

  // Touch pointers "leave" as the finger lifts, so only a mouse closes here;
  // touch popovers close on a tap elsewhere.
  const onPointerLeave = (e: React.PointerEvent) => {
    if (e.pointerType !== "mouse") return;
    window.clearTimeout(openTimer.current);
    closeTimer.current = window.setTimeout(() => setActive(null), CLOSE_GRACE);
  };

  // Touch and pen: the first tap on a segment shows its popover instead of
  // following the surrounding link; a tap on the same segment follows it.
  const onClick = (e: React.MouseEvent) => {
    if (lastPointer.current === "mouse") return;
    const key = segmentAt(e.clientX);
    if (key && key !== active) {
      e.preventDefault();
      setActive(key);
    }
  };

  const pct = (v: number) => `${Math.round((v / total) * 100)}%`;
  const summary =
    total === 0
      ? "No work items"
      : `${total} work items: ${visible.map((s) => `${s.label} ${s.value}`).join(", ")}`;
  const current = visible.find((s) => s.key === active);

  return (
    <div
      ref={barRef}
      className={[styles.bar, className].filter(Boolean).join(" ")}
      onPointerDown={(e) => (lastPointer.current = e.pointerType)}
      onPointerMove={total ? onPointerMove : undefined}
      onPointerLeave={total ? onPointerLeave : undefined}
      onClick={total ? onClick : undefined}
    >
      <span className="methods-sr-only">{summary}</span>
      <div className={styles.track} aria-hidden="true">
        {total === 0 ? (
          <span className={styles.empty} />
        ) : (
          visible.map((s) => (
            <span
              key={s.key}
              ref={(el) => {
                if (el) segRefs.current.set(s.key, el);
                else segRefs.current.delete(s.key);
              }}
              className={styles.segment}
              data-active={s.key === active || undefined}
              data-dimmed={(active && s.key !== active) || undefined}
              style={{ flexGrow: s.value, background: s.color }}
            />
          ))
        )}
      </div>
      {total > 0 && (
        <div
          ref={popRef}
          popover="manual"
          aria-hidden="true"
          className={styles.popover}
          style={current ? ({ "--seg": current.color } as CSSProperties) : undefined}
        >
          {current && (
            <>
              <span className={styles.swatch} />
              <span className={styles.label}>{current.label}</span>
              <strong className={styles.count}>{current.value}</strong>
              <span className={styles.pct}>{pct(current.value)}</span>
            </>
          )}
        </div>
      )}
    </div>
  );
}
