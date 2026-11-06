"use client";

import { useRef, useState } from "react";
import { api, errorMessage } from "@/lib/api";

/**
 * Opens a file chooser and uploads the chosen image. Render `input`
 * somewhere in the component; call `choose` from a button.
 */
export function useImageUpload(onUploaded: (fileId: string, url: string) => void) {
  const ref = useRef<HTMLInputElement>(null);
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
      onUploaded(uploaded.id, uploaded.url);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      if (ref.current) ref.current.value = "";
    }
  }

  const input = (
    <input
      ref={ref}
      type="file"
      accept="image/png,image/jpeg,image/gif,image/webp,image/avif"
      hidden
      onChange={(e) => pick(e.target.files?.[0])}
    />
  );
  return { input, choose: () => ref.current?.click(), busy, error };
}
