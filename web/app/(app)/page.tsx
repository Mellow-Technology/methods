"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { Alert } from "gui-mellow/Alert";
import { Button } from "gui-mellow/Button";
import { Container } from "gui-mellow/Container";
import { Dialog } from "gui-mellow/Dialog";
import { TextField } from "gui-mellow/Field";
import { PageHeader } from "gui-mellow/PageHeader";
import { ProjectForm, emptyProjectForm, projectFormBody, type ProjectFormValue } from "@/components/ProjectForm";
import { ProjectIcon } from "@/components/ProjectIcon";
import { StackedBar } from "@/components/StackedBar";
import { STATES, api, errorMessage, type Project, type Workspace } from "@/lib/api";
import { useLiveRefresh } from "@/lib/live";
import { useSession } from "@/lib/session";
import { toneColor } from "gui-mellow/utils";
import styles from "./projects.module.scss";

function ProjectCard({ project }: { project: Project }) {
  const counts = project.state_counts;
  const total = Object.values(counts).reduce((a, b) => a + (b ?? 0), 0);
  const open = (counts.backlog ?? 0) + (counts.todo ?? 0) + (counts.in_progress ?? 0);
  return (
    <Link href={`/project/?id=${project.id}`} className={styles.card} data-glow>
      <div
        className={styles.cover}
        style={project.cover_url ? { backgroundImage: `url(${project.cover_url})` } : undefined}
      />
      <div className={styles.body}>
        <ProjectIcon project={project} size={44} className={styles.icon} />
        <div className={styles.titleRow}>
          <h2 className={styles.name}>{project.name}</h2>
          <span className={styles.ident}>{project.identifier}</span>
        </div>
        {project.description && <p className={styles.description}>{project.description}</p>}
        <StackedBar
          className={styles.progress}
          segments={STATES.map((s) => ({
            key: s.value,
            label: s.label,
            value: counts[s.value] ?? 0,
            color: toneColor(s.tone),
          }))}
        />
        <p className={styles.meta}>
          {total === 0 ? "No work items yet" : `${open} open · ${counts.done ?? 0} done · ${total} total`}
        </p>
      </div>
    </Link>
  );
}

function CreateWorkspace() {
  const { refreshWorkspaces, selectWorkspace } = useSession();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <div className={styles.onboarding}>
      <h2>Create your first workspace</h2>
      <p>A workspace holds your projects, your team and your bots.</p>
      <form
        className={styles.inline}
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            const ws = await api.post<Workspace>("/workspaces", { name });
            await refreshWorkspaces();
            selectWorkspace(ws.id);
          } catch (err) {
            setError(errorMessage(err));
            setBusy(false);
          }
        }}
      >
        <TextField
          label="Workspace name"
          hideLabel
          placeholder="Acme Inc."
          value={name}
          onChange={(e) => setName(e.target.value)}
          error={error ?? undefined}
        />
        <Button type="submit" loading={busy} disabled={!name.trim()}>
          Create workspace
        </Button>
      </form>
    </div>
  );
}

export default function ProjectsPage() {
  const { workspace, refreshWorkspaces } = useSession();
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [form, setForm] = useState<ProjectFormValue>(emptyProjectForm);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!workspace) return;
    try {
      setProjects(await api.get<Project[]>(`/workspaces/${workspace.id}/projects`));
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [workspace]);

  useEffect(() => {
    setProjects(null);
    load();
  }, [load]);

  // Counts and cards follow changes made anywhere in the workspace.
  useLiveRefresh((e) => e.entity_type === "project" || e.entity_type === "work_item", load);

  async function create() {
    if (!workspace) return;
    setBusy(true);
    setFormError(null);
    try {
      await api.post(`/workspaces/${workspace.id}/projects`, projectFormBody(form));
      setCreating(false);
      setForm(emptyProjectForm);
      await Promise.all([load(), refreshWorkspaces()]);
    } catch (e) {
      setFormError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (!workspace) {
    return (
      <Container as="main" className={styles.page}>
        <CreateWorkspace />
      </Container>
    );
  }

  return (
    <Container as="main" size="wide" className={styles.page}>
      <PageHeader
        eyebrow={workspace.name}
        title="Projects"
        actions={<Button onClick={() => setCreating(true)}>New project</Button>}
      />
      {error && <Alert tone="danger">{error}</Alert>}
      {projects === null && !error && <div className="methods-loading">Loading projects…</div>}
      {projects?.length === 0 && (
        <div className={styles.onboarding}>
          <h2>No projects yet</h2>
          <p>Projects hold work items on a kanban board.</p>
          <Button onClick={() => setCreating(true)}>Create a project</Button>
        </div>
      )}
      {projects && projects.length > 0 && (
        <ul className={styles.grid}>
          {projects.map((p) => (
            <li key={p.id}>
              <ProjectCard project={p} />
            </li>
          ))}
        </ul>
      )}

      <Dialog
        open={creating}
        onClose={() => setCreating(false)}
        title="New project"
        size="md"
        actions={
          <>
            <Button variant="ghost" onClick={() => setCreating(false)}>
              Cancel
            </Button>
            <Button onClick={create} loading={busy} disabled={!form.name.trim() || !form.identifier.trim()}>
              Create project
            </Button>
          </>
        }
      >
        {formError && <Alert tone="danger">{formError}</Alert>}
        <ProjectForm value={form} onChange={setForm} autoIdentifier />
      </Dialog>
    </Container>
  );
}
