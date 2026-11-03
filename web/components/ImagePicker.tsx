"use client";

import { useRef, useState } from "react";
import { Button } from "gui-mellow/Button";
import { api, errorMessage } from "@/lib/api";
import styles from "./ImagePicker.module.scss";

interface Props {
  label: string;
  url: string | null;
  /** Called with the uploaded file's id, or null when removed. */
  onChange: (fileId: string | null, url: string | null) => void;
  shape?: "cover" | "square" | "round";
}

/** Uploads an image to object storage and previews it. */
export function ImagePicker({ label, url, onChange, shape = "square" }: Props) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function pick(file: File | undefined) {
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      setError("Choose an image file");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const uploaded = await api.upload(file);
      onChange(uploaded.id, uploaded.url);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  }

  return (
    <div className={styles.picker}>
      <span className={styles.label}>{label}</span>
      <div className={styles.row}>
        <button
          type="button"
          className={`${styles.preview} ${styles[shape]}`}
          onClick={() => input.current?.click()}
          aria-label={`Upload ${label.toLowerCase()}`}
        >
          {url ? <img src={url} alt="" /> : <span>+</span>}
        </button>
        <div className={styles.actions}>
          <Button size="sm" variant="secondary" loading={busy} onClick={() => input.current?.click()}>
            {url ? "Replace" : "Upload"}
          </Button>
          {url && (
            <Button size="sm" variant="ghost" onClick={() => onChange(null, null)}>
              Remove
            </Button>
          )}
        </div>
      </div>
      {error && <p className={styles.error}>{error}</p>}
      <input
        ref={input}
        type="file"
        accept="image/png,image/jpeg,image/gif,image/webp,image/avif"
        hidden
        onChange={(e) => pick(e.target.files?.[0])}
      />
    </div>
  );
}
