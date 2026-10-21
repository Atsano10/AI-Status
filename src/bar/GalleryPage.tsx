// Dev-only page that draws every bar state with sample data (entry: gallery-main.tsx).
import { useEffect, useState, type CSSProperties } from "react";
import { BarView } from "./Bar";
import Panel from "./Panel";
import Pill from "./Pill";
import type { Prefs } from "./prefs";
import Critter from "./Critter";
import {
  critterMood,
  derivePill,
  lastActivity,
  type Mood,
  type Orientation,
} from "./state";
import type { AgentId, LimitWindow, UsageSnapshot } from "./types";

const NOW = Math.floor(Date.now() / 1000);
const H = 3600;

function win(
  minutes: number,
  pct: number,
  resetsIn: number,
  approximate = false,
): LimitWindow {
  return {
    used_pct: pct,
    window_minutes: minutes,
    resets_at: NOW + resetsIn,
    observed_at: NOW - 120,
    approximate,
  };
}
const claude = (
  fiveH: number,
  week = 41,
  resetsIn = 4.2 * H,
): UsageSnapshot => ({
  agent: "Claude",
  windows: [win(300, fiveH, resetsIn, true), win(10080, week, 2.6 * 24 * H)],
});
const codex: UsageSnapshot = {
  agent: "Codex",
  windows: [win(43200, 6, 22 * 24 * H)],
};
const base: Prefs = {
  orientation: "horizontal",
  barShows: "5h",
  pausedUntil: null,
};

interface Sample {
  name: string;
  note: string;
  snapshots: UsageSnapshot[];
  agent: AgentId | null;
  prefs?: Partial<Prefs>;
}
const SAMPLES: Sample[] = [
  {
    name: "Normal",
    note: "< 75%",
    snapshots: [claude(23), codex],
    agent: "Claude",
  },
  {
    name: "Close",
    note: "75–90%",
    snapshots: [claude(82), codex],
    agent: "Claude",
  },
  {
    name: "Almost out",
    note: "> 90%",
    snapshots: [claude(94), codex],
    agent: "Claude",
  },
  {
    name: "Limit hit",
    note: "100% · countdown",
    snapshots: [claude(100, 63, 42 * 60), codex],
    agent: "Claude",
  },
  {
    name: "Just reset",
    note: "celebrate",
    snapshots: [claude(71, 41, -90), codex],
    agent: "Claude",
  },
  {
    name: "Codex",
    note: "30-day window",
    snapshots: [claude(23), codex],
    agent: "Codex",
  },
  {
    name: "No data",
    note: "bridge not installed",
    snapshots: [codex],
    agent: "Claude",
  },
  {
    name: "No agent",
    note: "nothing running",
    snapshots: [claude(23), codex],
    agent: null,
  },
  {
    name: "Paused",
    note: "tracking off",
    snapshots: [claude(23), codex],
    agent: "Claude",
    prefs: { pausedUntil: "manual" },
  },
];

const SF =
  '-apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif';
const SF_ROUNDED =
  'ui-rounded, "SF Pro Rounded", -apple-system, system-ui, sans-serif';

/** Font candidates: `ui` for the text, `num` for the numbers. */
const FONTS: { name: string; note: string; ui: string; num: string }[] = [
  {
    name: "A · Current",
    note: "SF Pro + SF Pro Rounded numbers",
    ui: SF,
    num: SF_ROUNDED,
  },
  {
    name: "B · All rounded",
    note: "SF Pro Rounded everywhere",
    ui: SF_ROUNDED,
    num: SF_ROUNDED,
  },
  {
    name: "C · Nunito",
    note: "soft and friendly (the Figma stand-in)",
    ui: '"Nunito", sans-serif',
    num: '"Nunito", sans-serif',
  },
  {
    name: "D · Geist",
    note: "clean and modern",
    ui: '"Geist Sans", sans-serif',
    num: '"Geist Sans", sans-serif',
  },
  {
    name: "E · JetBrains Mono",
    note: "terminal look, like the agents",
    ui: '"JetBrains Mono", monospace',
    num: '"JetBrains Mono", monospace',
  },
  {
    name: "F · Pixelify",
    note: "pixel font to match the critters",
    ui: '"Pixelify Sans", sans-serif',
    num: '"Pixelify Sans", sans-serif',
  },
];

const MOODS: Mood[] = [
  "idle",
  "working",
  "nervous",
  "alarm",
  "cheer",
  "sleep",
  "confused",
];

export default function GalleryPage() {
  const [orientation, setOrientation] = useState<Orientation>("horizontal");
  const [live, setLive] = useState(0);

  return (
    <div className="gallery">
      <h1>StatusPal bar</h1>
      <p className="gallery-sub">
        Every pill state, every critter mood, then the panel. The app uses these
        same components; this page just feeds them sample data.
      </p>

      <section>
        <h2>Pill states</h2>
        <div className="gallery-states">
          {(["horizontal", "vertical"] as const).map((o) => (
            <div key={o} className={`gallery-row gallery-row--${o}`}>
              {SAMPLES.map((s) => {
                const snap = s.agent
                  ? (s.snapshots.find((x) => x.agent === s.agent) ?? null)
                  : null;
                const p = { ...base, ...s.prefs };
                const view = derivePill(
                  snap,
                  s.agent,
                  p.barShows,
                  p.pausedUntil !== null,
                  NOW,
                );
                return (
                  <figure key={s.name}>
                    <Pill
                      view={view}
                      orientation={o}
                      agent={s.agent}
                      mood={critterMood(view, lastActivity(snap), NOW)}
                    />
                    {o === "horizontal" && (
                      <figcaption>
                        <b>{s.name}</b> {s.note}
                      </figcaption>
                    )}
                  </figure>
                );
              })}
            </div>
          ))}
        </div>
      </section>

      <section>
        <h2>Fonts</h2>
        <p className="gallery-sub">
          The same pill and panel in each candidate font. Pick a letter.
        </p>
        <div className="gallery-fonts">
          {FONTS.map((f) => {
            const sample = SAMPLES[0];
            const snap = sample.snapshots[0];
            const view = derivePill(snap, sample.agent, "5h", false, NOW);
            return (
              <figure
                key={f.name}
                className="gallery-font"
                style={
                  {
                    "--font-ui": f.ui,
                    "--font-num": f.num,
                  } as CSSProperties
                }
              >
                <figcaption>
                  <b>{f.name}</b> {f.note}
                </figcaption>
                <div className="stage stage--horizontal">
                  <Pill
                    view={view}
                    orientation="horizontal"
                    agent={sample.agent}
                    mood="idle"
                  />
                  <Panel
                    snapshots={sample.snapshots}
                    agent={sample.agent}
                    now={NOW}
                    prefs={base}
                    paused={false}
                    onPrefs={() => {}}
                    onHide={() => {}}
                    onQuit={() => {}}
                    closing={false}
                  />
                </div>
              </figure>
            );
          })}
        </div>
      </section>

      <section>
        <h2>Critters</h2>
        <div className="gallery-critters">
          {(["Claude", "Codex"] as const).map((agent) => (
            <div key={agent} className="gallery-row">
              {MOODS.map((mood) => (
                <figure key={mood} className="gallery-critter">
                  <Critter agent={agent} mood={mood} />
                  <figcaption>
                    <b>{mood}</b> {agent}
                  </figcaption>
                </figure>
              ))}
            </div>
          ))}
        </div>
      </section>

      <section>
        <h2>Panel</h2>
        <div className="gallery-panels">
          {[SAMPLES[0], SAMPLES[3], SAMPLES[5], SAMPLES[6], SAMPLES[8]].map(
            (s) => (
              <figure key={s.name}>
                <Panel
                  snapshots={s.snapshots}
                  agent={s.agent}
                  now={NOW}
                  prefs={{ ...base, ...s.prefs }}
                  paused={s.prefs?.pausedUntil != null}
                  onPrefs={() => {}}
                  onHide={() => {}}
                  onQuit={() => {}}
                  closing={false}
                />
                <figcaption>
                  <b>{s.name}</b>
                </figcaption>
              </figure>
            ),
          )}
        </div>
      </section>

      <section>
        <h2>Try it</h2>
        <p className="gallery-sub">
          Click the pill to open the panel, click again to close it. The ☰ menu
          and pause buttons work here too.
        </p>
        <div className="gallery-controls">
          <select
            value={live}
            onChange={(e) => setLive(Number(e.target.value))}
          >
            {SAMPLES.map((s, i) => (
              <option key={s.name} value={i}>
                {s.name}
              </option>
            ))}
          </select>
          <select
            value={orientation}
            onChange={(e) => setOrientation(e.target.value as Orientation)}
          >
            <option value="horizontal">Horizontal</option>
            <option value="vertical">Vertical</option>
          </select>
        </div>
        <div className="gallery-desktop">
          <div className="gallery-notch" />
          <LiveBar
            key={`${live}-${orientation}`}
            sample={SAMPLES[live]}
            orientation={orientation}
            onOrientation={setOrientation}
          />
        </div>
      </section>
    </div>
  );
}

/** A BarView with its own prefs, so the menu and pause buttons do something. Remounted per sample. */
function LiveBar(props: {
  sample: Sample;
  orientation: Orientation;
  onOrientation: (o: Orientation) => void;
}) {
  const [prefs, setPrefs] = useState<Prefs>({
    ...base,
    ...props.sample.prefs,
    orientation: props.orientation,
  });
  const [now, setNow] = useState(NOW);
  useEffect(() => {
    const timer = setInterval(() => setNow((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, []);
  return (
    <div
      className={`gallery-live gallery-live--${props.orientation === "vertical" ? "side" : "top"}`}
    >
      <BarView
        snapshots={props.sample.snapshots}
        agent={props.sample.agent}
        now={now}
        prefs={prefs}
        onPrefs={(change) => {
          if (change.orientation) props.onOrientation(change.orientation);
          setPrefs((p) => ({ ...p, ...change }));
        }}
      />
    </div>
  );
}
