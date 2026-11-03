"use client";

import { useEffect, useRef, useState } from "react";
import { Alert } from "gui-mellow/Alert";
import { Button } from "gui-mellow/Button";
import { Dialog } from "gui-mellow/Dialog";
import { SelectField, TextField } from "gui-mellow/Field";
import {
  PRIORITIES,
  STATES,
  api,
  errorMessage,
  type Epic,
  type Member,
  type Priority,
  type State,
  type WorkItemDetail,
} from "@/lib/api";
import { RichTextEditor, type RichTextHandle } from "./RichTextEditor";
import styles from "./NewItemDialog.module.scss";

interface Props {
  projectId: string;
  open: boolean;
  initialState: State;
  members: Member[];
  epics: Epic[];
  onClose: () => void;
  onCreated: (item: WorkItemDetail, keepOpen: boolean) => void;
}

export function NewItemDialog({ projectId, open, initialState, members, epics, onClose, onCreated }: Props) {
  const [title, setTitle] = useState("");
  const [state, setState] = useState<State>(initialState);
  const [priority, setPriority] = useState<Priority>("medium");
  const [assignee, setAssignee] = useState("");
  const [epic, setEpic] = useState("");
  const [due, setDue] = useState("");
  const [another, setAnother] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const description = useRef<RichTextHandle>(null);

  useEffect(() => {
    if (open) {
      setState(initialState);
      setError(null);
    }
  }, [open, initialState]);

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const item = await api.post<WorkItemDetail>(`/projects/${projectId}/work-items`, {
        title,
        state,
        priority,
        description: description.current?.getDoc() ?? null,
        assignee_id: assignee || null,
        epic_id: epic || null,
        due_date: due || null,
      });
      onCreated(item, another);
      setTitle("");
      description.current?.clear();
      if (!another) {
        setAssignee("");
        setEpic("");
        setDue("");
        setPriority("medium");
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="New work item"
      size="lg"
      actions={
        <>
          <label className={styles.another}>
            <input type="checkbox" checked={another} onChange={(e) => setAnother(e.target.checked)} /> Create another
          </label>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={create} loading={busy} disabled={!title.trim()}>
            Create
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        {error && <Alert tone="danger">{error}</Alert>}
        <TextField
          label="Title"
          required
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && title.trim() && (e.preventDefault(), create())}
        />
        <RichTextEditor ref={description} label="Description" placeholder="Description (optional)" minHeight="5rem" />
        <div className={styles.grid}>
          <SelectField label="State" value={state} options={STATES} onChange={(e) => setState(e.target.value as State)} />
          <SelectField
            label="Priority"
            value={priority}
            options={PRIORITIES}
            onChange={(e) => setPriority(e.target.value as Priority)}
          />
          <SelectField label="Assignee" value={assignee} onChange={(e) => setAssignee(e.target.value)}>
            <option value="">Unassigned</option>
            {members.map((m) => (
              <option key={m.id} value={m.id}>
                {m.kind === "bot" ? "🤖 " : ""}
                {m.display_name}
              </option>
            ))}
          </SelectField>
          <SelectField label="Epic" value={epic} onChange={(e) => setEpic(e.target.value)}>
            <option value="">No epic</option>
            {epics.map((e) => (
              <option key={e.id} value={e.id}>
                {e.name}
              </option>
            ))}
          </SelectField>
          <TextField label="Due date" type="date" value={due} onChange={(e) => setDue(e.target.value)} />
        </div>
      </div>
    </Dialog>
  );
}
