import type { CSSProperties, MouseEventHandler } from "react";
import Critter from "./Critter";
import { until, type Mood, type Orientation, type PillView } from "./state";
import type { AgentId } from "./types";

interface PillProps {
  view: PillView;
  orientation: Orientation;
  /** Whose critter to draw. `null` shows the paw placeholder. */
  agent: AgentId | null;
  mood: Mood;
  /** Mouse down on the pill: a click toggles the panel, a press-and-move drags the window (see Bar.tsx). */
  onMouseDown?: MouseEventHandler;
}

/** The floating bar itself: critter │ track │ window + %. Matches the "Pill" component in Figma. */
export default function Pill({
  view,
  orientation,
  agent,
  mood,
  onMouseDown,
}: PillProps) {
  const vertical = orientation === "vertical";
  const pct = fillPct(view);
  const level =
    view.kind === "usage"
      ? view.level
      : view.kind === "limit-hit"
        ? "out"
        : null;

  return (
    <div
      className={`pill pill--${orientation} is-${view.kind}${level ? ` level-${level}` : ""}`}
      style={{ "--pct": pct } as CSSProperties}
      onMouseDown={onMouseDown}
    >
      <span className="critter-slot" aria-hidden>
        {agent ? <Critter agent={agent} mood={mood} /> : <PawIcon />}
      </span>
      <span className="track" aria-hidden>
        <span className="fill" />
      </span>
      <Label view={view} vertical={vertical} />
    </div>
  );
}

function Label({ view, vertical }: { view: PillView; vertical: boolean }) {
  const { value, window } = labelText(view, vertical);
  const paused = view.kind === "paused";
  // Vertical stacks the value over the window so the text stays upright.
  return (
    <span className="label">
      {!vertical && window && <span className="label-window">{window}</span>}
      {paused && <PauseIcon />}
      <span className="label-value">{value}</span>
      {vertical && window && <span className="label-window">{window}</span>}
    </span>
  );
}

function labelText(
  view: PillView,
  vertical: boolean,
): { value: string; window: string } {
  switch (view.kind) {
    case "usage":
      return {
        value: `${view.approximate ? "~" : ""}${Math.round(view.pct)}%`,
        window: view.window,
      };
    case "limit-hit": {
      const left =
        view.resetsIn === null ? "100%" : until(view.resetsIn).split(" ")[0];
      return vertical
        ? { value: left, window: "reset" }
        : { value: `↻ ${left}`, window: view.window };
    }
    case "just-reset":
      return { value: "0%", window: view.window };
    case "paused":
      return {
        value: view.pct === null ? "" : `${Math.round(view.pct)}%`,
        window: vertical ? "" : view.window,
      };
    case "no-data":
      return vertical
        ? { value: "—", window: "setup" }
        : { value: "set up", window: "" };
    case "no-agent":
      return { value: "idle", window: "" };
  }
}

function fillPct(view: PillView): number {
  switch (view.kind) {
    case "usage":
      return Math.min(100, Math.max(0, view.pct));
    case "limit-hit":
      return 100;
    case "paused":
      return view.pct ?? 0;
    default:
      return 0;
  }
}

/** Shown when no agent is active yet. */
function PawIcon() {
  return (
    <svg width="34" height="34" viewBox="0 0 28 28" fill="none">
      <circle
        cx="14"
        cy="14"
        r="12.5"
        stroke="var(--surface-hairline)"
        strokeWidth="1.2"
        strokeDasharray="2.5 2.5"
      />
      <g fill="var(--text-tertiary)">
        <ellipse cx="14" cy="16.8" rx="4.2" ry="3.4" />
        <ellipse cx="8.8" cy="12.6" rx="1.6" ry="2" />
        <ellipse cx="11.9" cy="9.6" rx="1.6" ry="2.1" />
        <ellipse cx="16.1" cy="9.6" rx="1.6" ry="2.1" />
        <ellipse cx="19.2" cy="12.6" rx="1.6" ry="2" />
      </g>
    </svg>
  );
}

export function PauseIcon() {
  return (
    <svg
      className="icon-pause"
      width="8"
      height="9"
      viewBox="0 0 8 9"
      fill="currentColor"
      aria-label="paused"
    >
      <rect x="0.5" y="0" width="2.5" height="9" rx="1" />
      <rect x="5" y="0" width="2.5" height="9" rx="1" />
    </svg>
  );
}
