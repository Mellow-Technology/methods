"use client";

import type { ReactNode } from "react";
import { Button } from "gui-mellow/Button";
import { useImageUpload } from "./useImageUpload";
import styles from "./CoverField.module.scss";

/**
 * Edits a cover image in place: the cover as it will look, with its controls
 * along the top. `children` (usually an icon or avatar slot and its controls)
 * sits under the cover with the slot overlapping it, as on the real page.
 */
export function CoverField({
  url,
  onChange,
  children,
}: {
  url: string | null;
  onChange: (fileId: string | null, url: string | null) => void;
  children?: ReactNode;
}) {
  const upload = useImageUpload(onChange);
  return (
    <div className={styles.field}>
      <div className={styles.cover} style={url ? { backgroundImage: `url(${url})` } : undefined}>
        <div className={styles.coverActions}>
          <Button size="sm" variant="secondary" loading={upload.busy} onClick={upload.choose}>
            {url ? "Change cover" : "Add cover"}
          </Button>
          {url && (
            <Button size="sm" variant="secondary" onClick={() => onChange(null, null)}>
              Remove
            </Button>
          )}
        </div>
      </div>
      {children && <div className={styles.below}>{children}</div>}
      {upload.error && <p className={styles.error}>{upload.error}</p>}
      {upload.input}
    </div>
  );
}

/** A clickable icon or avatar that overlaps the cover above it. */
export function CoverSlot({
  label,
  onClick,
  round = false,
  children,
}: {
  label: string;
  onClick: () => void;
  round?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`${styles.slot} ${round ? styles.round : ""}`}
      onClick={onClick}
      aria-label={label}
      title={label}
    >
      {children}
      <span className={styles.slotHint} aria-hidden>
        Change
      </span>
    </button>
  );
}

export function CoverFieldError({ children }: { children: ReactNode }) {
  return <p className={styles.error}>{children}</p>;
}
