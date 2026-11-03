"use client";

import { useRouter } from "next/navigation";
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { ApiError, api, type User, type Workspace } from "./api";

const WORKSPACE_KEY = "methods.workspace";

interface Session {
  me: User;
  setMe: (u: User) => void;
  workspaces: Workspace[];
  workspace: Workspace | null;
  selectWorkspace: (id: string) => void;
  refreshWorkspaces: () => Promise<Workspace[]>;
  logout: () => Promise<void>;
}

const SessionContext = createContext<Session | null>(null);

export function useSession() {
  const s = useContext(SessionContext);
  if (!s) throw new Error("useSession outside SessionProvider");
  return s;
}

function storedWorkspace() {
  try {
    return localStorage.getItem(WORKSPACE_KEY);
  } catch {
    return null;
  }
}

/** Loads the signed-in user, or sends them to the login page. */
export function SessionProvider({ children, fallback }: { children: ReactNode; fallback: ReactNode }) {
  const router = useRouter();
  const [me, setMe] = useState<User | null>(null);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceId, setWorkspaceId] = useState<string | null>(null);

  const refreshWorkspaces = useCallback(async () => {
    const list = await api.get<Workspace[]>("/workspaces");
    setWorkspaces(list);
    setWorkspaceId((current) => {
      const wanted = current ?? storedWorkspace();
      return list.some((w) => w.id === wanted) ? wanted : (list[0]?.id ?? null);
    });
    return list;
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const user = await api.get<User>("/me");
        if (cancelled) return;
        setMe(user);
        await refreshWorkspaces();
      } catch (err) {
        if (err instanceof ApiError && err.status === 401) {
          const next = `${window.location.pathname}${window.location.search}`;
          router.replace(`/login/?next=${encodeURIComponent(next)}`);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // Only on first load; navigation within the app keeps the session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectWorkspace = useCallback((id: string) => {
    setWorkspaceId(id);
    try {
      localStorage.setItem(WORKSPACE_KEY, id);
    } catch {}
  }, []);

  const logout = useCallback(async () => {
    await api.post("/auth/logout");
    router.replace("/login/");
  }, [router]);

  const value = useMemo<Session | null>(
    () =>
      me && {
        me,
        setMe,
        workspaces,
        workspace: workspaces.find((w) => w.id === workspaceId) ?? null,
        selectWorkspace,
        refreshWorkspaces,
        logout,
      },
    [me, workspaces, workspaceId, selectWorkspace, refreshWorkspaces, logout],
  );

  if (!value) return <>{fallback}</>;
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
