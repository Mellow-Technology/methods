"use client";

import { Button } from "gui-mellow/Button";
import { TextField } from "gui-mellow/Field";
import type { Project } from "@/lib/api";
import { CoverField, CoverFieldError, CoverSlot } from "./CoverField";
import { useImageUpload } from "./useImageUpload";
import { ProjectIcon } from "./ProjectIcon";
import styles from "./ProjectForm.module.scss";

export interface ProjectFormValue {
  name: string;
  identifier: string;
  description: string;
  icon_emoji: string;
  icon_file_id: string | null;
  icon_url: string | null;
  cover_file_id: string | null;
  cover_url: string | null;
}

export const emptyProjectForm: ProjectFormValue = {
  name: "",
  identifier: "",
  description: "",
  icon_emoji: "",
  icon_file_id: null,
  icon_url: null,
  cover_file_id: null,
  cover_url: null,
};

export function projectToForm(p: Project): ProjectFormValue {
  return {
    name: p.name,
    identifier: p.identifier,
    description: p.description,
    icon_emoji: p.icon_emoji ?? "",
    icon_file_id: p.icon_file_id,
    icon_url: p.icon_url,
    cover_file_id: p.cover_file_id,
    cover_url: p.cover_url,
  };
}

export function projectFormBody(v: ProjectFormValue) {
  return {
    name: v.name,
    identifier: v.identifier,
    description: v.description,
    icon_emoji: v.icon_emoji.trim() || null,
    icon_file_id: v.icon_file_id,
    cover_file_id: v.cover_file_id,
  };
}

/** Suggests a project ID from its name: "Mobile App" → "MA", "Methods" → "MET". */
function suggestIdentifier(name: string) {
  const words = name.toUpperCase().replace(/[^A-Z0-9 ]/g, "").split(/\s+/).filter(Boolean);
  if (words.length === 0) return "";
  const id = words.length === 1 ? words[0]!.slice(0, 3) : words.map((w) => w[0]).join("").slice(0, 5);
  return /^[A-Z]/.test(id) ? id : `P${id}`.slice(0, 5);
}

/** The project's icon as it will appear, with its image and emoji controls. */
function IconControls({ value, onChange }: { value: ProjectFormValue; onChange: (v: ProjectFormValue) => void }) {
  const upload = useImageUpload((id, url) => onChange({ ...value, icon_file_id: id, icon_url: url }));
  const preview = {
    name: value.name,
    identifier: value.identifier || "?",
    icon_url: value.icon_url,
    icon_emoji: value.icon_emoji.trim() || null,
  };
  return (
    <>
      <CoverSlot label="Change icon" onClick={upload.choose}>
        <ProjectIcon project={preview} size={64} />
      </CoverSlot>
      <div className={styles.iconControls}>
        <Button size="sm" variant="secondary" loading={upload.busy} onClick={upload.choose}>
          {value.icon_url ? "Replace icon" : "Upload icon"}
        </Button>
        {value.icon_url && (
          <Button size="sm" variant="ghost" onClick={() => onChange({ ...value, icon_file_id: null, icon_url: null })}>
            Remove
          </Button>
        )}
        <TextField
          label="Emoji"
          hideLabel
          value={value.icon_emoji}
          maxLength={8}
          placeholder="or emoji 🚀"
          title={value.icon_url ? "Shown when there's no icon image" : undefined}
          className={styles.emoji}
          onChange={(e) => onChange({ ...value, icon_emoji: e.target.value })}
        />
      </div>
      {upload.error && <CoverFieldError>{upload.error}</CoverFieldError>}
      {upload.input}
    </>
  );
}

export function ProjectForm({
  value,
  onChange,
  autoIdentifier = false,
}: {
  value: ProjectFormValue;
  onChange: (v: ProjectFormValue) => void;
  /** Fill in the project ID from the name until it's edited by hand. */
  autoIdentifier?: boolean;
}) {
  const set = <K extends keyof ProjectFormValue>(key: K, v: ProjectFormValue[K]) => onChange({ ...value, [key]: v });

  return (
    <div className={styles.form}>
      <CoverField url={value.cover_url} onChange={(id, url) => onChange({ ...value, cover_file_id: id, cover_url: url })}>
        <IconControls value={value} onChange={onChange} />
      </CoverField>
      <div className={styles.row}>
        <TextField
          label="Name"
          required
          value={value.name}
          autoFocus
          className={styles.grow}
          onChange={(e) => {
            const name = e.target.value;
            const follow = autoIdentifier && value.identifier === suggestIdentifier(value.name);
            onChange({ ...value, name, identifier: follow ? suggestIdentifier(name) : value.identifier });
          }}
        />
        <TextField
          label="Project ID"
          required
          hint="Prefix for item keys"
          value={value.identifier}
          maxLength={10}
          className={styles.ident}
          controlClassName={styles.mono}
          onChange={(e) => set("identifier", e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, ""))}
        />
      </div>
      <TextField
        label="Description"
        multiline
        rows={3}
        value={value.description}
        onChange={(e) => set("description", e.target.value)}
      />
    </div>
  );
}
