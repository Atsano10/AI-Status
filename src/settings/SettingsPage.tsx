import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import type { BridgeStatus } from "../bindings/BridgeStatus";
import type { Orientation, PreferredWindow } from "../bar/state";
import type { AgentId } from "../bar/types";
import ShortcutRecorder from "./ShortcutRecorder";
import {
  updateSettings,
  useSettings,
  type Behaviour,
  type Settings,
} from "./useSettings";

/** The settings window (src-tauri/src/settings_window.rs). Every change saves straight away. */
export default function SettingsPage() {
  const settings = useSettings();
  if (settings === null) return null; // a few ms while they load

  return (
    <main className="settings">
      <Section title="Hotkey">
        <Row label="Show/hide bar" hint="Click, then press the keys you want.">
          <ShortcutRecorder hotkey={settings.hotkey} />
        </Row>
      </Section>

      <Section title="Bar">
        <Row label="When left alone" hint={behaviourHint(settings)}>
          <Choice<Behaviour>
            value={settings.behaviour}
            options={[
              ["toggle", "Stay"],
              ["fade", "Fade"],
              ["peek", "Hide"],
            ]}
            onChange={(behaviour) => updateSettings({ behaviour })}
          />
        </Row>
        {settings.behaviour === "peek" && (
          <Row label="Hide after">
            <Choice<string>
              value={String(settings.peekSeconds)}
              options={[
                ["2", "2 s"],
                ["4", "4 s"],
                ["8", "8 s"],
              ]}
              onChange={(s) => updateSettings({ peekSeconds: Number(s) })}
            />
          </Row>
        )}
        <Row label="Orientation">
          <Choice<Orientation>
            value={settings.orientation}
            options={[
              ["horizontal", "Horizontal"],
              ["vertical", "Vertical"],
            ]}
            onChange={(orientation) => updateSettings({ orientation })}
          />
        </Row>
        <Row
          label="Pill shows"
          hint="For agents with both a 5-hour and a weekly limit."
        >
          <Choice<PreferredWindow>
            value={settings.barShows}
            options={[
              ["5h", "5 hours"],
              ["week", "Week"],
            ]}
            onChange={(barShows) => updateSettings({ barShows })}
          />
        </Row>
      </Section>

      <Section title="Critter">
        <Row label="Gets moving" hint={energyHint(settings.gaitThresholds)}>
          <Choice<Energy>
            value={energyOf(settings.gaitThresholds)}
            options={[
              ["easily", "Easily"],
              ["normal", "Normal"],
              ["rarely", "Rarely"],
            ]}
            onChange={(e) =>
              updateSettings({
                gaitThresholds: ENERGY[e as Exclude<Energy, "custom">],
              })
            }
          />
        </Row>
      </Section>

      <Section title="Agent">
        <Row
          label="Pin agent"
          hint="Auto follows the app you're using. Also in the menu bar menu."
        >
          <Choice<"auto" | AgentId>
            value={settings.pinned ?? "auto"}
            options={[
              ["auto", "Auto"],
              ["Claude", "Claude"],
              ["Codex", "Codex"],
            ]}
            onChange={(pin) =>
              updateSettings({ pinned: pin === "auto" ? null : pin })
            }
          />
        </Row>
      </Section>

      <Section title="Claude Code">
        <ClaudeBridge />
      </Section>

      <Section title="General">
        <LaunchAtLogin />
      </Section>
    </main>
  );
}

/**
 * How easily the critter starts jogging and running: where its gait thresholds sit, in effective tokens per
 * 10 minutes (crates/core/src/activity.rs). Normal comes from your own usage: 150k is your quieter quarter,
 * 500k your busiest.
 */
type Energy = "easily" | "normal" | "rarely" | "custom";
const ENERGY: Record<Exclude<Energy, "custom">, Settings["gaitThresholds"]> = {
  easily: { jogAt: 50_000, runAt: 200_000 },
  normal: { jogAt: 150_000, runAt: 500_000 },
  rarely: { jogAt: 300_000, runAt: 1_000_000 },
};

function energyOf(t: Settings["gaitThresholds"]): Energy {
  const match = Object.entries(ENERGY).find(
    ([, preset]) => preset.jogAt === t.jogAt && preset.runAt === t.runAt,
  );
  return (match?.[0] as Energy | undefined) ?? "custom";
}

function energyHint(t: Settings["gaitThresholds"]): string {
  const k = (n: number) =>
    n >= 1_000_000 ? `${n / 1_000_000}M` : `${n / 1000}k`;
  return `Jogs from ${k(t.jogAt)} tokens in 10 minutes, runs from ${k(t.runAt)}.`;
}

function behaviourHint(settings: Settings): string {
  switch (settings.behaviour) {
    case "toggle":
      return "Stays until you press the hotkey again.";
    case "fade":
      return "Fades after a few seconds. Hover to wake it.";
    case "peek":
      return `Hides after ${settings.peekSeconds} seconds. Hovering keeps it up.`;
  }
}

/**
 * Connects StatusPal to Claude Code's status line (src-tauri/src/bridge.rs). That edits
 * ~/.claude/settings.json, so Connect always asks first.
 */
function ClaudeBridge() {
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Check again whenever the window comes forward, in case the file was edited by hand.
    const check = () =>
      invoke<BridgeStatus>("get_bridge_status").then(setStatus);
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);

  const set = (connected: boolean) =>
    invoke<BridgeStatus>("set_bridge_connected", { connected })
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setConfirming(false));

  if (status === null) return null;

  if (confirming)
    return (
      <Row
        label="Connect Claude Code?"
        hint={
          status === "otherStatusLine"
            ? "This edits ~/.claude/settings.json (a backup is saved next to it). Your current status line keeps showing."
            : "This edits ~/.claude/settings.json (a backup is saved next to it first)."
        }
      >
        <div className="settings-buttons">
          <button
            className="settings-button"
            onClick={() => setConfirming(false)}
          >
            Cancel
          </button>
          <button
            className="settings-button settings-button--primary"
            onClick={() => set(true)}
          >
            Connect
          </button>
        </div>
      </Row>
    );

  return (
    <Row label="Claude limits" hint={error ?? bridgeHint(status)}>
      {status === "connected" ? (
        <div className="settings-buttons">
          <span className="settings-status">
            <span className="settings-status-dot" /> Connected
          </span>
          <button className="settings-button" onClick={() => set(false)}>
            Remove
          </button>
        </div>
      ) : (
        <button
          className="settings-button settings-button--primary"
          onClick={() => setConfirming(true)}
        >
          {status === "broken" ? "Reconnect" : "Connect"}
        </button>
      )}
    </Row>
  );
}

function bridgeHint(status: BridgeStatus): string {
  switch (status) {
    case "connected":
      return "Claude Code sends StatusPal your limits after every reply.";
    case "notConnected":
      return "Adds StatusPal as Claude Code's status line, so the bar gets your Claude limits.";
    case "otherStatusLine":
      return "Claude Code already has a status line. StatusPal keeps it and reads the limits alongside.";
    case "broken":
      return "StatusPal moved since it was connected. Reconnect to fix it.";
  }
}

/** Open at login (src-tauri/src/login.rs). macOS keeps this one, not our settings file. */
function LaunchAtLogin() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  useEffect(() => {
    invoke<boolean>("get_launch_at_login").then(setEnabled);
  }, []);
  return (
    <Row
      label="Open at login"
      hint="Start StatusPal when you log in to your Mac."
    >
      <Switch
        checked={enabled ?? false}
        disabled={enabled === null}
        onChange={(on) =>
          invoke<boolean>("set_launch_at_login", { enabled: on }).then(
            setEnabled,
          )
        }
      />
    </Row>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h2 className="settings-section-title">{title}</h2>
      <div className="settings-card">{children}</div>
    </section>
  );
}

function Row(props: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-row-label">{props.label}</span>
        {props.hint && <span className="settings-row-hint">{props.hint}</span>}
      </div>
      {props.children}
    </div>
  );
}

function Choice<T extends string>(props: {
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented settings-choice">
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
  );
}

function Switch(props: {
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={props.checked}
      disabled={props.disabled}
      className={`switch${props.checked ? " is-on" : ""}`}
      onClick={() => props.onChange(!props.checked)}
    >
      <span className="switch-knob" />
    </button>
  );
}
