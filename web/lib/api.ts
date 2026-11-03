// A thin client for the Methods API. The API is same-origin (proxied in dev,
// hosted by the Rust server in production), so the session cookie just works.

import type { RichDoc } from "./richtext";

import type { Tone } from "gui-mellow";

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const init: RequestInit = { method, credentials: "same-origin" };
  if (body instanceof FormData) {
    init.body = body;
  } else if (body !== undefined) {
    init.headers = { "content-type": "application/json" };
    init.body = JSON.stringify(body);
  }
  const res = await fetch(`/api${path}`, init);
  if (res.status === 204 || res.status === 202) return undefined as T;
  const text = await res.text();
  let data: unknown;
  try {
    data = text ? JSON.parse(text) : undefined;
  } catch {
    data = undefined;
  }
  if (!res.ok) {
    const message = (data as { error?: string } | undefined)?.error ?? `Request failed (${res.status})`;
    throw new ApiError(res.status, message);
  }
  return data as T;
}

export const api = {
  get: <T>(path: string) => request<T>("GET", path),
  post: <T>(path: string, body?: unknown) => request<T>("POST", path, body ?? {}),
  patch: <T>(path: string, body: unknown) => request<T>("PATCH", path, body),
  del: (path: string) => request<void>("DELETE", path),
  upload: (file: File) => {
    const form = new FormData();
    form.append("file", file, file.name);
    return request<FileOut>("POST", "/files", form);
  },
};

export function errorMessage(err: unknown) {
  return err instanceof Error ? err.message : "Something went wrong";
}

// ---- Types -----------------------------------------------------------------

export type Uuid = string;

export interface AuthConfig {
  password_login: boolean;
  allow_registration: boolean;
  oidc: { name: string } | null;
}

export interface User {
  id: Uuid;
  kind: "human" | "bot";
  display_name: string;
  first_name: string | null;
  last_name: string | null;
  email: string | null;
  avatar_file_id: Uuid | null;
  avatar_url: string | null;
  cover_file_id: Uuid | null;
  cover_url: string | null;
  created_at: string;
}

export interface Workspace {
  id: Uuid;
  name: string;
  slug: string;
  role: "owner" | "admin" | "member";
  project_count: number;
  created_at: string;
}

export interface Member {
  id: Uuid;
  kind: "human" | "bot";
  display_name: string;
  email: string | null;
  avatar_url: string | null;
  role: "owner" | "admin" | "member";
}

export interface Bot {
  id: Uuid;
  display_name: string;
  avatar_url: string | null;
  workspace_id: Uuid;
  active_tokens: number;
  last_used_at: string | null;
  created_at: string;
}

export interface BotToken {
  id: Uuid;
  name: string;
  token_prefix: string;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
}

export interface Project {
  id: Uuid;
  workspace_id: Uuid;
  name: string;
  identifier: string;
  description: string;
  icon_emoji: string | null;
  icon_file_id: Uuid | null;
  icon_url: string | null;
  cover_file_id: Uuid | null;
  cover_url: string | null;
  state_counts: Partial<Record<State, number>>;
  created_at: string;
  updated_at: string;
}

export interface Label {
  id: Uuid;
  project_id: Uuid;
  name: string;
  color: string;
}

export interface Epic {
  id: Uuid;
  project_id: Uuid;
  name: string;
  description: string;
  color: string;
  item_count: number;
  done_count: number;
  created_at: string;
}

export interface WorkItem {
  id: Uuid;
  project_id: Uuid;
  seq: number;
  key: string;
  title: string;
  state: State;
  priority: Priority;
  assignee_id: Uuid | null;
  assignee_name: string | null;
  assignee_kind: "human" | "bot" | null;
  assignee_avatar_url: string | null;
  created_by: Uuid | null;
  start_date: string | null;
  due_date: string | null;
  parent_id: Uuid | null;
  epic_id: Uuid | null;
  sort_order: number;
  label_ids: Uuid[];
  comment_count: number;
  attachment_count: number;
  child_count: number;
  created_at: string;
  updated_at: string;
}

export interface ItemRef {
  id: Uuid;
  key: string;
  title: string;
  state: State;
  priority: Priority;
}

export interface Relation {
  id: Uuid;
  kind: RelationView;
  item_id: Uuid;
  item_key: string;
  item_title: string;
  item_state: State;
  item_priority: Priority;
}

export interface Link {
  id: Uuid;
  url: string;
  title: string | null;
  created_at: string;
}

export interface FileOut {
  id: Uuid;
  url: string;
  file_name: string;
  content_type: string;
  size_bytes: number;
  created_at: string;
}

export interface Attachment extends Omit<FileOut, "id"> {
  id: Uuid;
  file_id: Uuid;
  created_by_name: string | null;
}

export interface ReactionGroup {
  emoji: string;
  count: number;
  reacted: boolean;
  users: string[];
}

export interface WorkItemDetail extends WorkItem {
  /** TipTap JSON document, or null for no description. */
  description: RichDoc | null;
  created_by_name: string | null;
  labels: Label[];
  epic: Epic | null;
  parent: ItemRef | null;
  children: ItemRef[];
  relations: Relation[];
  links: Link[];
  attachments: Attachment[];
  reactions: ReactionGroup[];
}

export interface Comment {
  id: Uuid;
  work_item_id: Uuid;
  author_id: Uuid | null;
  author_name: string | null;
  author_kind: "human" | "bot" | null;
  author_avatar_url: string | null;
  /** TipTap JSON document. */
  body: RichDoc;
  created_at: string;
  updated_at: string;
}

// ---- History and live updates ---------------------------------------------

/** One field's change. Ids come with readable `*_label`s; rich text as a Markdown excerpt. */
export interface Change {
  from?: unknown;
  to?: unknown;
  from_label?: unknown;
  to_label?: unknown;
  rich?: boolean;
}

/** "X did Y to Z at time." */
export interface ActivityEntry {
  id: number;
  workspace_id: Uuid;
  project_id: Uuid | null;
  work_item_id: Uuid | null;
  related_item_id: Uuid | null;
  actor_id: Uuid | null;
  actor_name: string;
  actor_kind: "human" | "bot";
  actor_avatar_url: string | null;
  action: string;
  entity_type: string;
  entity_id: Uuid;
  entity_label: string;
  changes: Record<string, Change>;
  reverts: number | null;
  reverted_by: number | null;
  created_at: string;
  revertible: boolean;
}

export interface DescriptionVersion {
  activity_id: number;
  actor_name: string;
  actor_kind: "human" | "bot";
  actor_avatar_url: string | null;
  action: string;
  created_at: string;
  description: RichDoc | null;
}

/** A change announced over the live stream. `id` is null for changes with no history entry (reordering). */
export interface LiveEvent {
  id: number | null;
  workspace_id: Uuid;
  project_id: Uuid | null;
  work_item_id: Uuid | null;
  related_item_id: Uuid | null;
  entity_type: string;
  entity_id: Uuid;
  action: string;
  actor_id: Uuid | null;
}

// ---- Enumerations ----------------------------------------------------------

export type State = "backlog" | "todo" | "in_progress" | "done" | "cancelled";
export type Priority = "urgent" | "high" | "medium" | "low";
export type RelationKind = "relates_to" | "duplicate_of" | "blocked_by" | "blocks";
export type RelationView = RelationKind | "duplicated_by";

export const STATES: { value: State; label: string; tone: Tone }[] = [
  { value: "backlog", label: "Backlog", tone: "neutral" },
  { value: "todo", label: "Todo", tone: "primary" },
  { value: "in_progress", label: "In Progress", tone: "warning" },
  { value: "done", label: "Done", tone: "success" },
  { value: "cancelled", label: "Cancelled", tone: "danger" },
];

export const PRIORITIES: { value: Priority; label: string }[] = [
  { value: "urgent", label: "Urgent" },
  { value: "high", label: "High" },
  { value: "medium", label: "Medium" },
  { value: "low", label: "Low" },
];

export const RELATION_KINDS: { value: RelationKind; label: string }[] = [
  { value: "relates_to", label: "Relates to" },
  { value: "blocks", label: "Blocks" },
  { value: "blocked_by", label: "Blocked by" },
  { value: "duplicate_of", label: "Duplicate of" },
];

export const RELATION_LABELS: Record<RelationView, string> = {
  relates_to: "Relates to",
  blocks: "Blocks",
  blocked_by: "Blocked by",
  duplicate_of: "Duplicate of",
  duplicated_by: "Duplicated by",
};

export const stateInfo = (s: State) => STATES.find((x) => x.value === s) ?? STATES[0]!;
export const priorityLabel = (p: Priority) => PRIORITIES.find((x) => x.value === p)?.label ?? p;

export function formatDate(d: string | null) {
  if (!d) return "";
  // Date-only strings are calendar dates; don't shift them by timezone.
  const date = d.length === 10 ? new Date(`${d}T00:00:00`) : new Date(d);
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}

export function timeAgo(iso: string) {
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d ago`;
  return formatDate(iso);
}

export function formatBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
