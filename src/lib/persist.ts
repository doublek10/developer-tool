import { Dispatch, SetStateAction, useEffect, useRef, useState } from "react";

const PREFIX = "dw.session.";

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

/**
 * Behaves exactly like useState, except the value is written to the app's
 * local storage (debounced) and read back the next time this key is used —
 * whether that's navigating back to the module a minute later or opening
 * DevWorkstation again tomorrow. This is what lets a half-finished CV draft,
 * an open Code Editor tab, or an in-progress Database Lab repair still be
 * there to pick back up.
 *
 * This is per-device, per-install storage — it is not synced anywhere and
 * does not touch the SQLite database. It's meant for in-progress editing
 * state, not for anything that already has its own "Save" action recording
 * it durably (a saved CV, a saved project). Never pass a secret, password,
 * or vault value through this — those must never sit in local storage.
 */
export function usePersistedState<T>(
  key: string,
  initial: T
): [T, Dispatch<SetStateAction<T>>] {
  const [state, setState] = useState<T>(() => load(key, initial));
  const timer = useRef<number | null>(null);
  const first = useRef(true);

  useEffect(() => {
    // Skip the write on first mount — we just loaded this value, writing it
    // straight back is wasted work (and would stamp a fresh empty default
    // over a real session if load() ever raced storage).
    if (first.current) { first.current = false; return; }
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      try { localStorage.setItem(PREFIX + key, JSON.stringify(state)); }
      catch { /* storage full or unavailable — the in-memory session still works fine */ }
    }, 250);
    return () => { if (timer.current) window.clearTimeout(timer.current); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state]);

  return [state, setState];
}

/** Clears one persisted key — e.g. after a workflow finishes and its
 * in-progress state shouldn't be offered back on the next visit. */
export function clearPersisted(key: string) {
  try { localStorage.removeItem(PREFIX + key); } catch { /* ignore */ }
}
