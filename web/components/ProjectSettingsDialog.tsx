"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { Alert } from "gui-mellow/Alert";
import { Button } from "gui-mellow/Button";
import { ConfirmDialog, Dialog } from "gui-mellow/Dialog";
import { TextField } from "gui-mellow/Field";
import { Tabs } from "gui-mellow/Tabs";
import { api, errorMessage, type Epic, type Label, type Project } from "@/lib/api";
import { ProjectForm, projectFormBody, projectToForm, type ProjectFormValue } from "./ProjectForm";
import styles from "./ProjectSettingsDialog.module.scss";

const PALETTE = ["#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#0ea5e9", "#6366f1", "#a855f7", "#ec4899", "#64748b"];

interface Props {
  open: boolean;
  project: Project;
  labels: Label[];
  epics: Epic[];
  canDelete: boolean;
  onClose: () => void;
  onProject: (p: Project) => void;
  onLabels: (l: Label[]) => void;
  onEpics: (e: Epic[]) => void;
}

function ColorSwatches({ value, onChange }: { value: string; onChange: (c: string) => void }) {
  return (
    <span className={styles.swatches} role="radiogroup" aria-label="Colour">
      {PALETTE.map((c) => (
        <button
          key={c}
          type="button"
          role="radio"
          aria-checked={value === c}
          aria-label={c}
          className={styles.swatch}
          style={{ background: c }}
          onClick={() => onChange(c)}
        />
      ))}
    </span>
  );
}

export function ProjectSettingsDialog({ open, project, labels, epics, canDelete, onClose, onProject, onLabels, onEpics }: Props) {
  const router = useRouter();
  const [form, setForm] = useState<ProjectFormValue>(projectToForm(project));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [labelName, setLabelName] = useState("");
  const [labelColor, setLabelColor] = useState(PALETTE[5]!);
  const [epicName, setEpicName] = useState("");
  const [epicColor, setEpicColor] = useState(PALETTE[6]!);
  const [confirmDelete, setConfirmDelete] = useState(false);

  useEffect(() => {
    if (open) {
      setForm(projectToForm(project));
      setError(null);
    }
  }, [open, project]);

  const run = async (fn: () => Promise<void>) => {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const details = (
    <div className={styles.tab}>
      <ProjectForm value={form} onChange={setForm} />
      <div className={styles.actions}>
        {canDelete && (
          <Button variant="danger" onClick={() => setConfirmDelete(true)}>
            Delete project
          </Button>
        )}
        <Button
          loading={busy}
          onClick={() =>
            run(async () => {
              setBusy(true);
              try {
                onProject(await api.patch<Project>(`/projects/${project.id}`, projectFormBody(form)));
                onClose();
              } finally {
                setBusy(false);
              }
            })
          }
        >
          Save changes
        </Button>
      </div>
    </div>
  );

  const labelsTab = (
    <div className={styles.tab}>
      <ul className={styles.list}>
        {labels.map((l) => (
          <li key={l.id} className={styles.row}>
            <span className={styles.dot} style={{ background: l.color }} />
            <span className={styles.name}>{l.name}</span>
            <button
              type="button"
              className={styles.remove}
              aria-label={`Delete ${l.name}`}
              onClick={() =>
                run(async () => {
                  await api.del(`/labels/${l.id}`);
                  onLabels(labels.filter((x) => x.id !== l.id));
                })
              }
            >
              ×
            </button>
          </li>
        ))}
        {labels.length === 0 && <li className={styles.muted}>No labels yet.</li>}
      </ul>
      <form
        className={styles.add}
        onSubmit={(e) => {
          e.preventDefault();
          run(async () => {
            const l = await api.post<Label>(`/projects/${project.id}/labels`, { name: labelName, color: labelColor });
            onLabels([...labels, l].sort((a, b) => a.name.localeCompare(b.name)));
            setLabelName("");
          });
        }}
      >
        <TextField label="New label" value={labelName} onChange={(e) => setLabelName(e.target.value)} placeholder="bug" />
        <ColorSwatches value={labelColor} onChange={setLabelColor} />
        <Button type="submit" size="sm" disabled={!labelName.trim()}>
          Add label
        </Button>
      </form>
    </div>
  );

  const epicsTab = (
    <div className={styles.tab}>
      <ul className={styles.list}>
        {epics.map((e) => (
          <li key={e.id} className={styles.row}>
            <span className={styles.dot} style={{ background: e.color, borderRadius: 3 }} />
            <span className={styles.name}>{e.name}</span>
            <span className={styles.muted}>
              {e.done_count}/{e.item_count} done
            </span>
            <button
              type="button"
              className={styles.remove}
              aria-label={`Delete ${e.name}`}
              onClick={() =>
                run(async () => {
                  await api.del(`/epics/${e.id}`);
                  onEpics(epics.filter((x) => x.id !== e.id));
                })
              }
            >
              ×
            </button>
          </li>
        ))}
        {epics.length === 0 && <li className={styles.muted}>No epics yet. Epics group related work items.</li>}
      </ul>
      <form
        className={styles.add}
        onSubmit={(ev) => {
          ev.preventDefault();
          run(async () => {
            const epic = await api.post<Epic>(`/projects/${project.id}/epics`, { name: epicName, color: epicColor });
            onEpics([...epics, epic]);
            setEpicName("");
          });
        }}
      >
        <TextField label="New epic" value={epicName} onChange={(e) => setEpicName(e.target.value)} placeholder="Onboarding" />
        <ColorSwatches value={epicColor} onChange={setEpicColor} />
        <Button type="submit" size="sm" disabled={!epicName.trim()}>
          Add epic
        </Button>
      </form>
    </div>
  );

  return (
    <>
      <Dialog open={open} onClose={onClose} title="Project settings" size="lg">
        {error && <Alert tone="danger">{error}</Alert>}
        <Tabs
          label="Project settings"
          tabs={[
            { value: "details", label: "Details", content: details },
            { value: "labels", label: `Labels · ${labels.length}`, content: labelsTab },
            { value: "epics", label: `Epics · ${epics.length}`, content: epicsTab },
          ]}
        />
      </Dialog>
      <ConfirmDialog
        open={confirmDelete}
        title={`Delete ${project.name}?`}
        destructive
        confirmLabel="Delete project"
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() =>
          run(async () => {
            await api.del(`/projects/${project.id}`);
            router.replace("/");
          })
        }
      >
        Every work item, comment and attachment in this project will be deleted. This can't be undone.
      </ConfirmDialog>
    </>
  );
}
