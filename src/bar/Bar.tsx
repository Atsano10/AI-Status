import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import Panel from "./Panel";
import Pill from "./Pill";
import { isPaused, usePrefs, type Prefs } from "./prefs";
import { critterMood, derivePill, lastActivity } from "./state";
import type { AgentId, UsageSnapshot } from "./types";

/** The real bar: live data from Rust drawn by BarView. */
export default function Bar() {
  const snapshots = useSnapshots();
  const active = useActiveAgent();
  const now = useNow();
  const [prefs, setPrefs] = usePrefs();
  const shown = useShowCount();
  const hovered = useTauriEvent<boolean>("bar-hover", false);

  // Pausing freezes the numbers where they were, so nothing moves until you resume.
  const paused = prefs !== null && isPaused(prefs, now);
  const [frozen, setFrozen] = useState(snapshots);
  if (!paused && frozen !== snapshots) setFrozen(snapshots);
  const data = paused ? frozen : snapshots;

  if (data === null || prefs === null) return null; // first load, a few ms

  return (
    <BarView
      key={shown} // a fresh mount each time the hotkey shows the bar, so the peek-in animation replays
      snapshots={data}
      agent={active}
      now={now}
      prefs={prefs}
      onPrefs={setPrefs}
      hovered={hovered}
      onClickThrough={setClickThrough}
      onResize={resizeBar}
      onDragStart={() => getCurrentWindow().startDragging()}
      onHide={() => invoke("hide_bar")}
      onSettings={() => invoke("open_settings")}
      onQuit={() => invoke("quit")}
    />
  );
}

// Defined once, outside the component, so BarView's effects don't re-run on every render (the bar re-renders
// every second for the countdowns).
const setClickThrough = (enabled: boolean) =>
  invoke("set_click_through", { enabled });
const resizeBar = (width: number, height: number) =>
  invoke("resize_bar", { width, height });

interface BarViewProps {
  snapshots: UsageSnapshot[];
  agent: AgentId | null;
  now: number;
  prefs: Prefs;
  onPrefs: (change: Partial<Prefs>) => void;
  /**
   * Whether the mouse is over the bar, as seen by Rust. The webview gets no mouse-leave events while the bar
   * isn't focused (and it never is), so the app passes this in. Leave it out (the dev gallery) to use the
   * page's own mouse events.
   */
  hovered?: boolean;
  /** Called with `true` when the bar fades (clicks should pass through it) and `false` when it wakes. */
  onClickThrough?: (enabled: boolean) => void;
  onResize?: (width: number, height: number) => void;
  onDragStart?: () => void;
  onHide?: () => void;
  onSettings?: () => void;
  onQuit?: () => void;
}

const EXIT_ANIMATION = 140;
/** A press that moves this far (px), or is held this long (ms), drags the bar instead of clicking it. */
const DRAG_DISTANCE = 4;
const DRAG_HOLD = 220;
/** Left alone this long (ms), the bar fades so it's there when you glance at it but out of the way. */
const FADE_AFTER = 5000;
/** After something worth noticing (the critter's mood or agent changes), stay bright this long (ms). */
const ALERT_FOR = 8000;
/** Matches the fade-out transition in bar.css, so clicks pass through only once it's faded. */
const FADE_OUT_MS = 600;

/** Pill + panel, without any Tauri calls, so the dev gallery can show it with sample data. */
export function BarView(props: BarViewProps) {
  const {
    snapshots,
    agent,
    now,
    prefs,
    onPrefs,
    hovered,
    onClickThrough,
    onResize,
    onDragStart,
  } = props;
  const [domHover, setDomHover] = useState(false);
  // Click the pill to open the panel, click again to close it.
  const [open, setOpen] = useState(false);
  const panel = usePresence(open, EXIT_ANIMATION);

  const stage = useRef<HTMLDivElement>(null);
  const paused = isPaused(prefs, now);

  const snapshot = agent
    ? (snapshots.find((s) => s.agent === agent) ?? null)
    : null;
  const view = derivePill(snapshot, agent, prefs.barShows, paused, now);
  const mood = critterMood(view, lastActivity(snapshot), now);

  // Fade when left alone. Never while the panel is open or the mouse is over the bar.
  const faded = useFaded(open || (hovered ?? domHover), `${agent}|${mood}`);
  useEffect(() => {
    if (!onClickThrough) return;
    if (!faded) {
      onClickThrough(false); // straight away, so a hover can be followed by a click
      return;
    }
    const timer = setTimeout(() => onClickThrough(true), FADE_OUT_MS);
    return () => clearTimeout(timer);
  }, [faded, onClickThrough]);

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
      className={`stage stage--${prefs.orientation}${faded ? " is-faded" : ""}`}
      onMouseEnter={() => setDomHover(true)}
      onMouseLeave={() => setDomHover(false)}
    >
      <Pill
        view={view}
        orientation={prefs.orientation}
        agent={agent}
        mood={mood}
        onMouseDown={(e) => {
          if (e.button === 0)
            pressPill(e.nativeEvent, {
              onClick: () => setOpen((o) => !o),
              onDrag: () => onDragStart?.(),
            });
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
          onSettings={props.onSettings}
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

/**
 * Whether the bar should be faded. It fades once it's been left alone for `FADE_AFTER`, and wakes up while
 * `inUse` (hovered or panel open). A change in `alertKey` (a new mood or agent) wakes it for `ALERT_FOR`,
 * so a faded bar never sleeps through you running out.
 */
function useFaded(inUse: boolean, alertKey: string): boolean {
  const [idle, setIdle] = useState(false);
  const [delay, setDelay] = useState(FADE_AFTER);
  const [seen, setSeen] = useState({ inUse, alertKey });
  // Wake up during render when something changed, so there's never a frame with the old state.
  if (seen.inUse !== inUse || seen.alertKey !== alertKey) {
    setDelay(seen.alertKey !== alertKey ? ALERT_FOR : FADE_AFTER);
    setSeen({ inUse, alertKey });
    setIdle(false);
  }
  useEffect(() => {
    if (inUse) return;
    const timer = setTimeout(() => setIdle(true), delay);
    return () => clearTimeout(timer);
  }, [inUse, alertKey, delay]);
  return idle && !inUse;
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

/**
 * Tells a click from a drag. A press that moves a few pixels, or is held a moment, becomes a drag. Anything
 * shorter is a click. Once the drag starts, macOS takes over the mouse, so we stop listening.
 */
function pressPill(
  down: MouseEvent,
  handlers: { onClick: () => void; onDrag: () => void },
) {
  const startDrag = () => {
    stop();
    handlers.onDrag();
  };
  const move = (e: MouseEvent) => {
    const dx = e.screenX - down.screenX;
    const dy = e.screenY - down.screenY;
    if (Math.hypot(dx, dy) >= DRAG_DISTANCE) startDrag();
  };
  const up = () => {
    stop();
    handlers.onClick();
  };
  const timer = setTimeout(startDrag, DRAG_HOLD);
  function stop() {
    clearTimeout(timer);
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", up);
  }
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
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
