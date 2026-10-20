import { useState, type CSSProperties } from "react";
import { PauseIcon } from "./Pill";
import { tomorrowMorning, type Prefs } from "./prefs";
import {
  currentPct,
  levelFor,
  longLabel,
  resetText,
  shortLabel,
  updatedText,
  type Orientation,
  type PreferredWindow,
} from "./state";
import type { AgentId, LimitWindow, UsageSnapshot } from "./types";

interface PanelProps {
  snapshots: UsageSnapshot[];
  agent: AgentId | null;
  now: number;
  prefs: Prefs;
  paused: boolean;
  onPrefs: (change: Partial<Prefs>) => void;
  onHide: () => void;
  onQuit: () => void;
  /** Plays the exit animation before the panel is removed. */
  closing: boolean;
}

/** The hover view under the pill: every limit window with its reset time, pause, and the ☰ menu. */
export default function Panel(props: PanelProps) {
  const { snapshots, agent, now, prefs, paused, onPrefs, closing } = props;
  const [view, setView] = useState<"main" | "menu" | "pause">("main");
  const active = agent ? snapshots.find((s) => s.agent === agent) : undefined;
  const others = snapshots.filter((s) => s.agent !== agent);

  return (
    <div className={`panel${closing ? " is-closing" : ""}`}>
      {view === "menu" ? (
        <Menu {...props} />
      ) : (
        <>
          <Header agent={agent} snapshot={active} now={now} paused={paused} />
          {agent && active && active.windows.length > 0 && (
            <div className="panel-windows">
              {[...active.windows]
                .sort((a, b) => a.window_minutes - b.window_minutes)
                .map((w) => (
                  <WindowRow
                    key={w.window_minutes}
                    w={w}
                    now={now}
                    paused={paused}
                  />
                ))}
            </div>
          )}
          {agent && (!active || active.windows.length === 0) && (
            <NoData agent={agent} />
          )}
          {!agent && (
            <p className="panel-note">
              Switch to Claude, Codex, or a terminal running one.
            </p>
          )}
          {others.length > 0 && (
            <div className="panel-others">
              {others.map((s) => (
                <OtherAgent key={s.agent} snapshot={s} now={now} />
              ))}
            </div>
          )}
        </>
      )}

      {view === "pause" && (
        <div className="pause-options">
          <span className="pause-options-title">Pause tracking for…</span>
          <div className="segmented">
            <button
              onClick={() => (
                onPrefs({ pausedUntil: now + 3600 }),
                setView("main")
              )}
            >
              1 hour
            </button>
            <button
              onClick={() => (
                onPrefs({ pausedUntil: tomorrowMorning(now) }),
                setView("main")
              )}
            >
              Until tomorrow
            </button>
            <button
              onClick={() => (
                onPrefs({ pausedUntil: "manual" }),
                setView("main")
              )}
            >
              Until I resume
            </button>
          </div>
        </div>
      )}

      <div className="panel-divider" />
      <div className="panel-footer">
        {paused ? (
          <button
            className="btn"
            onClick={() => onPrefs({ pausedUntil: null })}
          >
            <PlayIcon /> Resume
            <span className="btn-hint">{pausedHint(prefs.pausedUntil)}</span>
          </button>
        ) : (
          <button
            className={`btn${view === "pause" ? " is-active" : ""}`}
            onClick={() => setView(view === "pause" ? "main" : "pause")}
          >
            <PauseIcon /> Pause tracking
          </button>
        )}
        <button
          className={`btn btn--icon${view === "menu" ? " is-active" : ""}`}
          aria-label="Menu"
          onClick={() => setView(view === "menu" ? "main" : "menu")}
        >
          <MenuIcon />
        </button>
      </div>
    </div>
  );
}

function Header({
  agent,
  snapshot,
  now,
  paused,
}: {
  agent: AgentId | null;
  snapshot: UsageSnapshot | undefined;
  now: number;
  paused: boolean;
}) {
  const observed = snapshot?.windows.reduce(
    (t, w) => Math.max(t, w.observed_at),
    0,
  );
  return (
    <div className="panel-header">
      <span className="panel-agent">
        <span
          className={`agent-dot agent-dot--${agent?.toLowerCase() ?? "none"}`}
        />
        {agent ?? "No agent detected"}
      </span>
      {paused ? (
        <span className="chip">Paused</span>
      ) : (
        observed !== undefined &&
        observed > 0 && (
          <span className="panel-meta">{updatedText(observed, now)}</span>
        )
      )}
    </div>
  );
}

function WindowRow({
  w,
  now,
  paused,
}: {
  w: LimitWindow;
  now: number;
  paused: boolean;
}) {
  const pct = currentPct(w, now);
  const reset = w.resets_at !== null && w.resets_at <= now;
  const level = reset ? "reset" : paused ? "paused" : levelFor(pct);
  return (
    <div
      className={`window-row level-${level}`}
      style={{ "--pct": Math.min(100, pct) } as CSSProperties}
    >
      <div className="window-row-top">
        <span className="window-name">{longLabel(w)}</span>
        <span className="window-value">
          {w.approximate && !reset && pct < 100 ? "~" : ""}
          {Math.round(pct)}%
        </span>
      </div>
      <span className="track track--thin">
        <span className="fill" />
      </span>
      <span className="window-meta">{resetText(w, now)}</span>
    </div>
  );
}

function OtherAgent({
  snapshot,
  now,
}: {
  snapshot: UsageSnapshot;
  now: number;
}) {
  return (
    <div className="other-agent">
      <span className="panel-agent panel-agent--small">
        <span
          className={`agent-dot agent-dot--${snapshot.agent.toLowerCase()}`}
        />
        {snapshot.agent}
      </span>
      <span className="other-agent-windows">
        {snapshot.windows.map((w) => {
          const pct = currentPct(w, now);
          return (
            <span
              key={w.window_minutes}
              className={`other-window level-${levelFor(pct)}`}
            >
              {shortLabel(w)} <b>{Math.round(pct)}%</b>
            </span>
          );
        })}
      </span>
    </div>
  );
}

function NoData({ agent }: { agent: AgentId }) {
  return (
    <p className="panel-note">
      {agent === "Claude"
        ? "No Claude numbers yet. They show up after your next Claude Code reply, once the StatusPal statusLine is installed."
        : "No Codex numbers yet. They show up after your next Codex turn."}
    </p>
  );
}

function Menu({ snapshots, prefs, onPrefs, onHide, onQuit }: PanelProps) {
  // Only offer 5h ↔ week when some agent actually has both windows.
  const hasTwo = snapshots.some((s) => s.windows.length > 1);
  return (
    <div className="menu">
      <MenuChoice<Orientation>
        label="Orientation"
        value={prefs.orientation}
        options={[
          ["horizontal", "Horizontal"],
          ["vertical", "Vertical"],
        ]}
        onChange={(orientation) => onPrefs({ orientation })}
      />
      {hasTwo && (
        <MenuChoice<PreferredWindow>
          label="Pill shows"
          value={prefs.barShows}
          options={[
            ["5h", "5 hours"],
            ["week", "Week"],
          ]}
          onChange={(barShows) => onPrefs({ barShows })}
        />
      )}
      <div className="panel-divider" />
      <button className="menu-item" onClick={onHide}>
        Hide bar <span className="menu-hint">⌃⌥Space</span>
      </button>
      <button className="menu-item" onClick={onQuit}>
        Quit StatusPal <span className="menu-hint">⌘Q</span>
      </button>
    </div>
  );
}

function MenuChoice<T extends string>(props: {
  label: string;
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <div className="menu-choice">
      <span className="menu-label">{props.label}</span>
      <div className="segmented">
        {props.options.map(([value, text]) => (
          <button
            key={value}
            className={value === props.value ? "is-active" : ""}
            onClick={() => props.onChange(value)}
          >
            {text}
          </button>
        ))}
      </div>
    </div>
  );
}

function pausedHint(until: Prefs["pausedUntil"]): string {
  if (until === "manual" || until === null) return "";
  const time = new Date(until * 1000).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  return `paused until ${time}`;
}

function PlayIcon() {
  return (
    <svg
      width="9"
      height="10"
      viewBox="0 0 9 10"
      fill="currentColor"
      aria-hidden
    >
      <path d="M1 1.2v7.6a.7.7 0 0 0 1.05.6l6.3-3.8a.7.7 0 0 0 0-1.2L2.05.6A.7.7 0 0 0 1 1.2Z" />
    </svg>
  );
}

function MenuIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden>
      <path
        d="M2.5 4h9M2.5 7h9M2.5 10h9"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
      />
    </svg>
  );
}
