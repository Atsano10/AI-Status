import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import Panel from "./Panel";
import Pill from "./Pill";
import { isPaused, usePrefs, type Prefs } from "./prefs";
import { derivePill } from "./state";
import type { AgentId, UsageSnapshot } from "./types";

/** The real bar: live data from Rust drawn by BarView. */
export default function Bar() {
  const snapshots = useSnapshots();
  const active = useActiveAgent();
  const now = useNow();
  const [prefs, setPrefs] = usePrefs();
  const hovered = useTauriEvent<boolean>("bar-hover", false);
  const shown = useShowCount();

  // Pausing freezes the numbers where they were, so nothing moves until you resume.
  const paused = isPaused(prefs, now);
  const [frozen, setFrozen] = useState(snapshots);
  if (!paused && frozen !== snapshots) setFrozen(snapshots);
  const data = paused ? frozen : snapshots;

  if (data === null) return null; // first load, a few ms

  return (
    <BarView
      key={shown} // a fresh mount each time the hotkey shows the bar, so the peek-in animation replays
      snapshots={data}
      agent={active}
      now={now}
      prefs={prefs}
      onPrefs={setPrefs}
      externalHover={hovered}
      onResize={(width, height) => invoke("resize_bar", { width, height })}
      onDragStart={() => getCurrentWindow().startDragging()}
      onHide={() => invoke("hide_bar")}
      onQuit={() => invoke("quit")}
    />
  );
}

interface BarViewProps {
  snapshots: UsageSnapshot[];
  agent: AgentId | null;
  now: number;
  prefs: Prefs;
  onPrefs: (change: Partial<Prefs>) => void;
  /** Hover as seen by Rust. The webview gets no mouse-move events while the bar isn't focused (and it never is). */
  externalHover?: boolean;
  onResize?: (width: number, height: number) => void;
  onDragStart?: () => void;
  onHide?: () => void;
  onQuit?: () => void;
}

const OPEN_DELAY = 180;
const CLOSE_DELAY = 320;
const EXIT_ANIMATION = 140;

/** Pill + hover panel, without any Tauri calls, so the dev gallery can show it with sample data. */
export function BarView(props: BarViewProps) {
  const {
    snapshots,
    agent,
    now,
    prefs,
    onPrefs,
    externalHover = false,
    onResize,
    onDragStart,
  } = props;
  const [domHover, setDomHover] = useState(false);
  const open = useDelayed(externalHover || domHover, OPEN_DELAY, CLOSE_DELAY);
  const panel = usePresence(open, EXIT_ANIMATION);
  const stage = useRef<HTMLDivElement>(null);
  const paused = isPaused(prefs, now);

  const snapshot = agent
    ? (snapshots.find((s) => s.agent === agent) ?? null)
    : null;
  const view = derivePill(snapshot, agent, prefs.barShows, paused, now);

  // Keep the window exactly the size of what's drawn, so the empty space around it doesn't eat clicks.
  useLayoutEffect(() => {
    const el = stage.current;
    if (!el || !onResize) return;
    const fit = () =>
      onResize(Math.ceil(el.offsetWidth), Math.ceil(el.offsetHeight));
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => observer.disconnect();
  }, [onResize]);

  return (
    <div
      ref={stage}
      className={`stage stage--${prefs.orientation}`}
      onMouseEnter={() => setDomHover(true)}
      onMouseLeave={() => setDomHover(false)}
    >
      <Pill
        view={view}
        orientation={prefs.orientation}
        onMouseDown={(e) => {
          if (e.button === 0) onDragStart?.();
        }}
      />
      {panel.mounted && (
        <Panel
          snapshots={snapshots}
          agent={agent}
          now={now}
          prefs={prefs}
          paused={paused}
          onPrefs={onPrefs}
          onHide={() => props.onHide?.()}
          onQuit={() => props.onQuit?.()}
          closing={panel.closing}
        />
      )}
    </div>
  );
}

/** Snapshots from Rust: fetched on load and every time the bar is shown, then pushed on every change. */
function useSnapshots(): UsageSnapshot[] | null {
  const [snapshots, setSnapshots] = useState<UsageSnapshot[] | null>(null);
  useEffect(() => {
    const refresh = () =>
      invoke<UsageSnapshot[]>("get_limits")
        .then(setSnapshots)
        .catch(() => setSnapshots([]));
    refresh();
    // Rust emits this every time the hotkey shows the bar.
    const unlistenShown = listen("bar-shown", refresh);
    // And this whenever Claude or Codex writes new usage data (crates/core/src/watch.rs).
    const unlistenUpdated = listen<UsageSnapshot[]>("limits-updated", (e) =>
      setSnapshots(e.payload),
    );
    return () => {
      unlistenShown.then((stop) => stop());
      unlistenUpdated.then((stop) => stop());
    };
  }, []);
  return snapshots;
}

/** The agent you're using right now (src-tauri/src/active.rs), or null if we can't tell yet. */
function useActiveAgent(): AgentId | null {
  const [active, setActive] = useState<AgentId | null>(null);
  useEffect(() => {
    // Ask once in case we missed the first event, then follow changes.
    invoke<AgentId | null>("get_active_agent").then(setActive);
    const unlisten = listen<AgentId | null>("active-agent-changed", (e) =>
      setActive(e.payload),
    );
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);
  return active;
}

/** The latest payload of a Rust event. */
function useTauriEvent<T>(name: string, initial: T): T {
  const [value, setValue] = useState(initial);
  useEffect(() => {
    const unlisten = listen<T>(name, (e) => setValue(e.payload));
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [name]);
  return value;
}

/** Goes up by one every time the hotkey shows the bar. */
function useShowCount(): number {
  const [count, setCount] = useState(0);
  useEffect(() => {
    const unlisten = listen("bar-shown", () => setCount((c) => c + 1));
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);
  return count;
}

/** The current time in Unix seconds, updated every second so countdowns keep ticking while the bar is open. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now() / 1000), 1000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

/** `value`, but it only turns on after `onDelay` ms and off after `offDelay` ms. Stops the panel flickering open on a quick pass. */
function useDelayed(
  value: boolean,
  onDelay: number,
  offDelay: number,
): boolean {
  const [delayed, setDelayed] = useState(value);
  useEffect(() => {
    const timer = setTimeout(
      () => setDelayed(value),
      value ? onDelay : offDelay,
    );
    return () => clearTimeout(timer);
  }, [value, onDelay, offDelay]);
  return delayed;
}

/** Keeps something mounted for `exitMs` after it closes, so it can animate out. */
function usePresence(
  open: boolean,
  exitMs: number,
): { mounted: boolean; closing: boolean } {
  const [mounted, setMounted] = useState(open);
  if (open && !mounted) setMounted(true);
  useEffect(() => {
    if (open) return;
    const timer = setTimeout(() => setMounted(false), exitMs);
    return () => clearTimeout(timer);
  }, [open, exitMs]);
  return { mounted: mounted || open, closing: mounted && !open };
}
