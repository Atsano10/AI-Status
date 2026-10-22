// What the bar needs from your preferences. Orientation and 5h/week live in the settings file (shared with the
// settings window, see src/settings/useSettings.ts). Pausing is a moment-to-moment state, not a setting, so
// it stays in this webview's localStorage.
import { useCallback, useState } from "react";
import { updateSettings, useSettings } from "../settings/useSettings";
import type { Orientation, PreferredWindow } from "./state";

export interface Prefs {
  orientation: Orientation;
  /** Which window the pill shows when an agent has two. */
  barShows: PreferredWindow;
  /** Unix seconds when tracking resumes, `"manual"` for "until I resume", or null when not paused. */
  pausedUntil: number | "manual" | null;
}

const KEY = "statuspal.bar.prefs";

function loadPausedUntil(): Prefs["pausedUntil"] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}").pausedUntil ?? null;
  } catch {
    return null;
  }
}

/** The bar's prefs (null until the settings load) and a function that saves a change to the right place. */
export function usePrefs(): [Prefs | null, (change: Partial<Prefs>) => void] {
  const settings = useSettings();
  const [pausedUntil, setPausedUntil] = useState(loadPausedUntil);

  const update = useCallback((change: Partial<Prefs>) => {
    const { pausedUntil: paused, ...rest } = change;
    if (paused !== undefined) {
      setPausedUntil(paused);
      try {
        localStorage.setItem(KEY, JSON.stringify({ pausedUntil: paused }));
      } catch {
        // Storage can be unavailable; the pause still applies until the bar reloads.
      }
    }
    if (Object.keys(rest).length > 0) void updateSettings(rest);
  }, []);

  const prefs = settings && {
    orientation: settings.orientation,
    barShows: settings.barShows,
    pausedUntil,
  };
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
