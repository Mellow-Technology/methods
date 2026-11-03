"use client";

import { useEffect, useState } from "react";
import { Avatar } from "gui-mellow/Avatar";
import { Button } from "gui-mellow/Button";
import { Dialog } from "gui-mellow/Dialog";
import { api, errorMessage, timeAgo, type DescriptionVersion } from "@/lib/api";
import { RichText } from "./RichTextEditor";
import styles from "./DescriptionHistory.module.scss";

/**
 * Every saved version of a work item's description, with restore. Restoring
 * saves that version as a new edit, so it's itself undoable.
 */
export function DescriptionHistory({
  itemId,
  open,
  onClose,
  onRestore,
}: {
  itemId: string;
  open: boolean;
  onClose: () => void;
  onRestore: (v: DescriptionVersion) => Promise<void>;
}) {
  const [versions, setVersions] = useState<DescriptionVersion[] | null>(null);
  const [selected, setSelected] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setVersions(null);
    setSelected(0);
    api
      .get<DescriptionVersion[]>(`/work-items/${itemId}/description-history`)
      .then(setVersions)
      .catch((e) => setError(errorMessage(e)));
  }, [open, itemId]);

  const current = versions?.[selected];
  const recordedFromStart = versions?.at(-1)?.action === "created";

  return (
    <Dialog open={open} onClose={onClose} title="Description history" size="lg">
      {error && <p className={styles.error}>{error}</p>}
      {versions === null && !error && <p className={styles.muted}>Loading…</p>}
      {versions?.length === 0 && <p className={styles.muted}>No earlier versions have been recorded.</p>}
      {versions && versions.length > 0 && (
        <div className={styles.layout}>
          <ol className={styles.versions} aria-label="Versions">
            {versions.map((v, i) => (
              <li key={v.activity_id}>
                <button
                  type="button"
                  className={styles.version}
                  aria-current={i === selected || undefined}
                  onClick={() => setSelected(i)}
                >
                  <Avatar name={v.actor_name} src={v.actor_avatar_url ?? undefined} size={20} />
                  <span>
                    <strong>{v.actor_name}</strong>
                    <span className={styles.when}>
                      {i === 0 ? "Current · " : ""}
                      {timeAgo(v.created_at)}
                    </span>
                  </span>
                </button>
              </li>
            ))}
            {!recordedFromStart && <li className={styles.hint}>Earlier versions weren’t recorded.</li>}
          </ol>
          <div className={styles.preview}>
            {current?.description ? (
              <RichText doc={current.description} />
            ) : (
              <p className={styles.muted}>(empty)</p>
            )}
            {current && selected > 0 && (
              <div className={styles.actions}>
                <Button
                  size="sm"
                  loading={busy}
                  onClick={async () => {
                    setBusy(true);
                    setError(null);
                    try {
                      await onRestore(current);
                      onClose();
                    } catch (e) {
                      setError(errorMessage(e));
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Restore this version
                </Button>
              </div>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}
