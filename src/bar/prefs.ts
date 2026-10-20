// Bar preferences. Kept in the webview's localStorage for now; they move to the settings file
// (Tauri store plugin) when the settings window exists.
import { useCallback, useState } from "react";
import type { Orientation, PreferredWindow } from "./state";

export interface Prefs {
  orientation: Orientation;
  /** Which window the pill shows when an agent has two. */
  barShows: PreferredWindow;
  /** Unix seconds when tracking resumes, `"manual"` for "until I resume", or null when not paused. */
  pausedUntil: number | "manual" | null;
}

const KEY = "statuspal.bar.prefs";
const DEFAULTS: Prefs = {
  orientation: "horizontal",
  barShows: "5h",
  pausedUntil: null,
};

function load(): Prefs {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return DEFAULTS;
  }
}

export function usePrefs(): [Prefs, (change: Partial<Prefs>) => void] {
  const [prefs, setPrefs] = useState(load);
  const update = useCallback((change: Partial<Prefs>) => {
    setPrefs((old) => {
      const next = { ...old, ...change };
      try {
        localStorage.setItem(KEY, JSON.stringify(next));
      } catch {
        // Storage can be unavailable; the change still applies until the bar reloads.
      }
      return next;
    });
  }, []);
  return [prefs, update];
}

export function isPaused(prefs: Prefs, now: number): boolean {
  return (
    prefs.pausedUntil === "manual" ||
    (prefs.pausedUntil !== null && prefs.pausedUntil > now)
  );
}

/** Unix seconds for tomorrow 8:00 local time. */
export function tomorrowMorning(now: number): number {
  const d = new Date(now * 1000);
  d.setDate(d.getDate() + 1);
  d.setHours(8, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}
