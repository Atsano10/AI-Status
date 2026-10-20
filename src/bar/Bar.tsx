import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { LimitWindow, UsageSnapshot } from "./types";

// Placeholder bar: plain text until the real designs land in Phase 3.
export default function Bar() {
  const [snapshots, setSnapshots] = useState<UsageSnapshot[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const now = useNow();

  useEffect(() => {
    const refresh = () =>
      invoke<UsageSnapshot[]>("get_limits")
        .then((s) => {
          setSnapshots(s);
          setError(null);
        })
        .catch((e) => setError(String(e)));

    refresh();
    // Rust emits this every time the hotkey shows the bar.
    const unlisten = listen("bar-shown", refresh);
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  if (error) return <div className="bar">⚠️ {error}</div>;
  if (!snapshots) return <div className="bar">🐾 …</div>;
  if (snapshots.length === 0)
    return <div className="bar">🐾 No usage data yet</div>;

  return (
    <div className="bar">
      {snapshots.map((s) => (
        <span key={s.agent} className="agent">
          <b>{s.agent}</b>
          {s.windows.map((w) => (
            <span key={w.window_minutes}> {describe(w, now)}</span>
          ))}
        </span>
      ))}
    </div>
  );
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

/** "5h 4% · 4h 20m", or "5h ~6% · ~3h 59m" when approximate */
function describe(w: LimitWindow, now: number): string {
  const label =
    { 300: "5h", 10080: "wk", 43200: "30d" }[w.window_minutes] ??
    `${w.window_minutes}m`;
  // "~" when the source rounds its numbers (Claude Code is often ~1% low and a few minutes early).
  const approx = w.approximate ? "~" : "";
  if (w.resets_at !== null && w.resets_at <= now) return `${label} 0%`; // window has reset
  const pct = `${label} ${approx}${Math.round(w.used_pct)}%`;
  return w.resets_at === null
    ? pct
    : `${pct} · ${approx}${until(w.resets_at - now)}`;
}

function until(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return d > 0 ? `${d}d ${h}h` : `${h}h ${m}m`;
}
