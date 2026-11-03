"use client";

import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import type { LiveEvent } from "./api";

/**
 * Real-time updates: one Server-Sent Events stream per workspace
 * (`/api/workspaces/{id}/events`). Events say what changed; views decide
 * whether it concerns them and re-fetch. "resync" means events may have been
 * missed (a reconnect), so everything showing should re-fetch.
 */
export type LiveSignal = LiveEvent | "resync";
type Listener = (signal: LiveSignal) => void;
export type LiveStatus = "connecting" | "live" | "offline";

interface Live {
  subscribe: (l: Listener) => () => void;
  status: LiveStatus;
}

const LiveContext = createContext<Live | null>(null);

export function LiveProvider({ workspaceId, children }: { workspaceId: string | null; children: ReactNode }) {
  const listeners = useRef(new Set<Listener>());
  const [status, setStatus] = useState<LiveStatus>("connecting");
  const [live] = useState(() => ({
    subscribe: (l: Listener) => {
      listeners.current.add(l);
      return () => {
        listeners.current.delete(l);
      };
    },
  }));

  useEffect(() => {
    if (!workspaceId || typeof EventSource === "undefined") return;
    const emit = (s: LiveSignal) => listeners.current.forEach((l) => l(s));
    const source = new EventSource(`/api/workspaces/${workspaceId}/events`);
    let connectedBefore = false;
    setStatus("connecting");
    source.addEventListener("ready", () => {
      setStatus("live");
      // Anything could have changed while we were away.
      if (connectedBefore) emit("resync");
      connectedBefore = true;
    });
    source.addEventListener("change", (m) => {
      try {
        emit(JSON.parse((m as MessageEvent).data) as LiveEvent);
      } catch {
        // Ignore malformed events.
      }
    });
    source.addEventListener("resync", () => emit("resync"));
    // EventSource reconnects by itself; CLOSED means it gave up (e.g. we
    // were removed from the workspace).
    source.onerror = () => setStatus(source.readyState === EventSource.CLOSED ? "offline" : "connecting");
    return () => source.close();
  }, [workspaceId]);

  return <LiveContext.Provider value={{ ...live, status }}>{children}</LiveContext.Provider>;
}

export function useLiveStatus(): LiveStatus {
  return useContext(LiveContext)?.status ?? "offline";
}

/**
 * Re-fetches when a matching change arrives. Bursts are collapsed into one
 * refresh, and while `paused()` is true (e.g. mid-drag) the refresh waits.
 */
export function useLiveRefresh(
  matches: (e: LiveEvent) => boolean,
  refresh: () => void,
  options: { delay?: number; paused?: () => boolean } = {},
) {
  const live = useContext(LiveContext);
  const latest = useRef({ matches, refresh, paused: options.paused });
  latest.current = { matches, refresh, paused: options.paused };
  const delay = options.delay ?? 200;

  useEffect(() => {
    if (!live) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const fire = () => {
      if (latest.current.paused?.()) {
        timer = setTimeout(fire, 300);
        return;
      }
      timer = undefined;
      latest.current.refresh();
    };
    const unsubscribe = live.subscribe((s) => {
      if (s !== "resync" && !latest.current.matches(s)) return;
      if (timer === undefined) timer = setTimeout(fire, delay);
    });
    return () => {
      unsubscribe();
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [live, delay]);
}
