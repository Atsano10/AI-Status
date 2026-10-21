// Turns the raw snapshots from Rust into what the pill and panel draw. No React here, so it's easy to test.
import type { AgentId, LimitWindow, UsageSnapshot } from "./types";

export type Orientation = "horizontal" | "vertical";
export type Level = "normal" | "close" | "out";
/** Which window the pill shows when an agent has two (Claude: 5h + week). */
export type PreferredWindow = "5h" | "week";

/** How the critter acts. See critter.css for what each one looks like. */
export type Mood =
  "idle" | "working" | "nervous" | "alarm" | "cheer" | "sleep" | "confused";

/** How recently an agent must have written usage data for its critter to look busy. */
export const WORKING_SECS = 90;

/** What the pill shows. One case per state in the Figma "Pill" component. */
export type PillView =
  | {
      kind: "usage";
      level: Level;
      pct: number;
      window: string;
      approximate: boolean;
    }
  | { kind: "limit-hit"; window: string; resetsIn: number | null }
  | { kind: "just-reset"; window: string }
  | { kind: "paused"; pct: number | null; window: string }
  | { kind: "no-data" }
  | { kind: "no-agent" };

const FIVE_HOURS = 300;
const WEEK = 10080;

/** How long after a reset the pill keeps celebrating (glowing green). */
export const JUST_RESET_SECS = 15 * 60;

/** Short label for the pill: "5h", "wk", "30d". */
export function shortLabel(w: LimitWindow): string {
  return (
    { [FIVE_HOURS]: "5h", [WEEK]: "wk", 43200: "30d" }[w.window_minutes] ??
    `${w.window_minutes}m`
  );
}

/** Long label for the panel: "5 hours", "Week", "30 days". */
export function longLabel(w: LimitWindow): string {
  return (
    { [FIVE_HOURS]: "5 hours", [WEEK]: "Week", 43200: "30 days" }[
      w.window_minutes
    ] ?? `${w.window_minutes} min`
  );
}

export function levelFor(pct: number): Level {
  if (pct >= 90) return "out";
  if (pct >= 75) return "close";
  return "normal";
}

/** The window the pill shows: the preferred one if the agent has it, otherwise its first one. */
export function pickWindow(
  snapshot: UsageSnapshot,
  preferred: PreferredWindow,
): LimitWindow | null {
  const want = preferred === "week" ? WEEK : FIVE_HOURS;
  return (
    snapshot.windows.find((w) => w.window_minutes === want) ??
    snapshot.windows[0] ??
    null
  );
}

/** % used right now. After the reset time the reading is stale and the real value is 0. */
export function currentPct(w: LimitWindow, now: number): number {
  return w.resets_at !== null && w.resets_at <= now ? 0 : w.used_pct;
}

export function derivePill(
  snapshot: UsageSnapshot | null,
  agent: AgentId | null,
  preferred: PreferredWindow,
  paused: boolean,
  now: number,
): PillView {
  if (agent === null) return { kind: "no-agent" };
  const w = snapshot ? pickWindow(snapshot, preferred) : null;
  if (!w)
    return paused
      ? { kind: "paused", pct: null, window: "" }
      : { kind: "no-data" };

  const window = shortLabel(w);
  const pct = currentPct(w, now);
  if (paused) return { kind: "paused", pct, window };

  if (w.resets_at !== null && w.resets_at <= now) {
    return now - w.resets_at < JUST_RESET_SECS
      ? { kind: "just-reset", window }
      : {
          kind: "usage",
          level: "normal",
          pct: 0,
          window,
          approximate: w.approximate,
        };
  }
  if (pct >= 100) {
    return {
      kind: "limit-hit",
      window,
      resetsIn: w.resets_at === null ? null : w.resets_at - now,
    };
  }
  return {
    kind: "usage",
    level: levelFor(pct),
    pct,
    window,
    approximate: w.approximate,
  };
}

/** "4h 12m", "2d 3h", "42m" */
export function until(secs: number): string {
  const s = Math.max(0, secs);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${Math.max(m, 1)}m`;
}

/** "resets in 4h 12m" within a day, "resets Thu 9:00" within a week, otherwise "resets Nov 12". */
export function resetText(w: LimitWindow, now: number): string {
  if (w.resets_at === null) return "reset time unknown";
  if (w.resets_at <= now) return "fresh window, just reset";
  const approx = w.approximate ? "~" : "";
  const secs = w.resets_at - now;
  if (secs < 86400) return `resets in ${approx}${until(secs)}`;
  const date = new Date(w.resets_at * 1000);
  if (secs < 7 * 86400) {
    const day = date.toLocaleDateString(undefined, { weekday: "short" });
    const time = date.toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    });
    return `resets ${day} ${time}`;
  }
  return `resets ${date.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
}

/** "updated just now", "updated 3m ago" */
export function updatedText(observedAt: number, now: number): string {
  const secs = now - observedAt;
  return secs < 60 ? "updated just now" : `updated ${until(secs)} ago`;
}

/** When the agent last wrote usage data, from the newest reading in its snapshot. */
export function lastActivity(snapshot: UsageSnapshot | null): number | null {
  if (!snapshot || snapshot.windows.length === 0) return null;
  return Math.max(...snapshot.windows.map((w) => w.observed_at));
}

/** The critter's mood for what the pill shows. */
export function critterMood(
  view: PillView,
  lastActive: number | null,
  now: number,
): Mood {
  switch (view.kind) {
    case "limit-hit":
      return "alarm";
    case "just-reset":
      return "cheer";
    case "paused":
    case "no-agent":
      return "sleep";
    case "no-data":
      return "confused";
    case "usage":
      if (view.level !== "normal") return "nervous";
      return lastActive !== null && now - lastActive < WORKING_SECS
        ? "working"
        : "idle";
  }
}
