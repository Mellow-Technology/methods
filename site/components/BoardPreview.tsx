import { toneColor, vars, type Tone } from "gui-mellow/utils";
import { BotIcon } from "./Logo";
import styles from "./BoardPreview.module.scss";

interface Card {
  id: string;
  title: string;
  labels?: string[];
  who: string;
  bot?: boolean;
  due?: string;
}

const LABEL_TONES: Record<string, Tone> = { auth: "accent", web: "primary", server: "success" };

const COLUMNS: { name: string; tone: Tone; cards: Card[] }[] = [
  {
    name: "Todo",
    tone: "primary",
    cards: [
      { id: "MTH-21", title: "Email invitations for new members", labels: ["auth"], who: "OL" },
      { id: "MTH-24", title: "List view for large projects", labels: ["web"], who: "JP" },
    ],
  },
  {
    name: "In Progress",
    tone: "warning",
    cards: [
      { id: "MTH-12", title: "Purge trash after 30 days", labels: ["server"], who: "Claude", bot: true, due: "Fri" },
      { id: "MTH-18", title: "Searchable assignee picker", labels: ["web"], who: "KC" },
    ],
  },
  {
    name: "Done",
    tone: "success",
    cards: [
      { id: "MTH-7", title: "Sign in with OIDC", labels: ["auth"], who: "KC" },
      { id: "MTH-5", title: "Live updates over SSE", labels: ["server"], who: "Claude", bot: true },
    ],
  },
];

/**
 * A still picture of a Methods board, drawn in HTML so it follows the theme.
 * Decorative: screen readers get a one-line description instead.
 */
export function BoardPreview() {
  return (
    <figure className={styles.window}>
      <figcaption className="sr-only">
        A Methods kanban board with Todo, In Progress and Done columns. Some cards are assigned to people, others to
        the Claude bot.
      </figcaption>
      <div className={styles.bar} aria-hidden>
        <span className={styles.dot} />
        <span className={styles.dot} />
        <span className={styles.dot} />
        <span className={styles.crumb}>
          Mellow <span>/</span> <strong>Methods</strong> <code>MTH</code>
        </span>
        <span className={styles.live}>Live</span>
      </div>
      <div className={styles.board} aria-hidden>
        {COLUMNS.map((col) => (
          <div key={col.name} className={styles.column} style={vars({ "--tint": toneColor(col.tone) })}>
            <div className={styles.columnHead}>
              <span className={styles.ring} />
              {col.name}
              <span className={styles.count}>{col.cards.length}</span>
            </div>
            {col.cards.map((card) => (
              <div key={card.id} className={styles.card}>
                <div className={styles.key}>{card.id}</div>
                <div className={styles.title}>{card.title}</div>
                <div className={styles.meta}>
                  {card.labels?.map((l) => (
                    <span key={l} className={styles.label} style={vars({ "--label": toneColor(LABEL_TONES[l] ?? "neutral") })}>
                      {l}
                    </span>
                  ))}
                  {card.due && <span className={styles.due}>{card.due}</span>}
                  <span className={card.bot ? `${styles.avatar} ${styles.bot}` : styles.avatar} title={card.who}>
                    {card.bot ? <BotIcon size={12} /> : card.who}
                  </span>
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>
    </figure>
  );
}
