import { toneColor } from "gui-mellow/utils";
import { priorityLabel, stateInfo, type Priority, type State } from "@/lib/api";

/** Signal bars for low/medium/high, an exclamation mark for urgent. */
export function PriorityIcon({ priority, size = 14 }: { priority: Priority; size?: number }) {
  const label = `${priorityLabel(priority)} priority`;
  if (priority === "urgent") {
    return (
      <svg width={size} height={size} viewBox="0 0 16 16" role="img" aria-label={label}>
        <title>{label}</title>
        <rect width="16" height="16" rx="4" fill="var(--gm-color-danger)" />
        <rect x="7" y="3" width="2" height="7" rx="1" fill="white" />
        <rect x="7" y="11.5" width="2" height="2" rx="1" fill="white" />
      </svg>
    );
  }
  const level = { high: 3, medium: 2, low: 1 }[priority];
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" role="img" aria-label={label}>
      <title>{label}</title>
      {[0, 1, 2].map((i) => (
        <rect
          key={i}
          x={2 + i * 5}
          y={10 - i * 4}
          width="3"
          height={4 + i * 4}
          rx="1"
          fill={i < level ? "var(--gm-color-text)" : "var(--gm-color-border-strong)"}
        />
      ))}
    </svg>
  );
}

/** A ring that fills as work moves from backlog to done. */
export function StateIcon({ state, size = 14 }: { state: State; size?: number }) {
  const info = stateInfo(state);
  const color = toneColor(info.tone);
  const fill = { backlog: 0, todo: 0, in_progress: 0.5, done: 1, cancelled: 1 }[state];
  const r = 5;
  const c = 2 * Math.PI * r;
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" role="img" aria-label={info.label}>
      <title>{info.label}</title>
      <circle
        cx="8"
        cy="8"
        r="6.5"
        fill="none"
        stroke={color}
        strokeWidth="1.5"
        strokeDasharray={state === "backlog" ? "2 2" : undefined}
      />
      {fill > 0 && state !== "cancelled" && (
        <circle
          cx="8"
          cy="8"
          r={r / 2}
          fill="none"
          stroke={color}
          strokeWidth={r}
          strokeDasharray={`${(c / 2) * fill} ${c}`}
          transform="rotate(-90 8 8)"
        />
      )}
      {state === "cancelled" && (
        <path d="M5.5 5.5l5 5m0-5l-5 5" stroke={color} strokeWidth="1.5" strokeLinecap="round" />
      )}
    </svg>
  );
}

export function BotIcon({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" role="img" aria-label="Bot">
      <title>Bot</title>
      <rect x="2" y="5" width="12" height="9" rx="3" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="6" cy="9.5" r="1.2" fill="currentColor" />
      <circle cx="10" cy="9.5" r="1.2" fill="currentColor" />
      <path d="M8 2v3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}
