import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import type { BridgeStatus } from "../bindings/BridgeStatus";
import type { Corners } from "../bindings/Corners";
import type { GeminiPlan } from "../bindings/GeminiPlan";
import type { HookStatus } from "../bindings/HookStatus";
import type { Palette } from "../bindings/Palette";
import type { Surface } from "../bindings/Surface";
import type { Theme } from "../bindings/Theme";
import type { CodexSetup } from "../bindings/CodexSetup";
import type { HelperInfo } from "../bindings/HelperInfo";
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

      <Section title="Look">
        <Row
          label="Colours"
          stacked
          hint="The bar's colours and each critter's."
        >
          <Choice<Palette>
            value={settings.palette}
            options={[
              ["classic", "Classic"],
              ["arcade", "Arcade"],
              ["sunset", "Sunset"],
              ["cool", "Cool"],
              ["pastel", "Pastel"],
            ]}
            onChange={(palette) => updateSettings({ palette })}
          />
        </Row>
        <Row
          label="Theme"
          hint="System follows macOS: dark at night, light by day if you've set that."
        >
          <Choice<Theme>
            value={settings.theme}
            options={[
              ["dark", "Dark"],
              ["light", "Light"],
              ["system", "System"],
            ]}
            onChange={(theme) => updateSettings({ theme })}
          />
        </Row>
        <Row
          label="Surface"
          hint="Solid blends into the MacBook notch. See-through shows your wallpaper behind the bar."
        >
          <Choice<Surface>
            value={settings.surface}
            options={[
              ["solid", "Solid"],
              ["glass", "See-through"],
            ]}
            onChange={(surface) => updateSettings({ surface })}
          />
        </Row>
        <Row label="Buttons" hint="In the bar's panel.">
          <Choice<Corners>
            value={settings.buttons}
            options={[
              ["rounded", "Rounded"],
              ["pixel", "Pixel"],
            ]}
            onChange={(buttons) => updateSettings({ buttons })}
          />
        </Row>
        <Row label="Settings cards" hint="These boxes, here.">
          <Choice<Corners>
            value={settings.cards}
            options={[
              ["rounded", "Rounded"],
              ["pixel", "Pixel"],
            ]}
            onChange={(cards) => updateSettings({ cards })}
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
              ["Gemini", "Gemini"],
            ]}
            onChange={(pin) =>
              updateSettings({ pinned: pin === "auto" ? null : pin })
            }
          />
        </Row>
      </Section>

      <Section title="Tips">
        <Row
          label="Show tips"
          hint="Short hints in the panel, like when you're about to run out or a reply re-sends a huge context."
        >
          <Switch
            checked={settings.tips}
            onChange={(tips) => updateSettings({ tips })}
          />
        </Row>
        {Object.keys(settings.hiddenTips).length > 0 && (
          <Row
            label="Hidden tips"
            hint={`${Object.keys(settings.hiddenTips).length} hidden with Hide or Never.`}
          >
            <button
              className="settings-button"
              onClick={() => updateSettings({ hiddenTips: {} })}
            >
              Show again
            </button>
          </Row>
        )}
      </Section>

      <Section title="Claude Code">
        <ClaudeBridge />
        <PromptHints />
      </Section>

      <Section title="Claude Code helpers">
        <Helpers />
      </Section>

      <Section title="Codex">
        <CodexSettings />
      </Section>

      <Section title="Gemini">
        <Row
          label="Your plan"
          hint={`Gemini CLI saves no %, so StatusPal counts today's requests against your plan's daily allowance (it resets at midnight Pacific time).${settings.geminiPlan === "auto" ? " Auto goes by how you sign in: a Google account counts as 1,000 a day." : ""}`}
        >
          <select
            className="settings-select"
            value={settings.geminiPlan}
            onChange={(e) =>
              updateSettings({ geminiPlan: e.target.value as GeminiPlan })
            }
          >
            {GEMINI_PLANS.map(([plan, name]) => (
              <option key={plan} value={plan}>
                {name}
              </option>
            ))}
          </select>
        </Row>
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

/**
 * The prompt classifier's hook (crates/core/src/adapters/claude/hook.rs, classify.rs). It edits
 * ~/.claude/settings.json, so Connect asks first, like the bridge.
 */
function PromptHints() {
  const [status, setStatus] = useState<HookStatus | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const check = () =>
      invoke<HookStatus>("get_prompt_hook_status").then(setStatus);
    check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);
  const set = (on: boolean) =>
    invoke<HookStatus>("set_prompt_hook", { on })
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
        label="Turn on prompt hints?"
        hint="This adds a hook to ~/.claude/settings.json (a backup is saved next to it, and your own hooks stay). It looks at each prompt on your Mac and saves only its guess, never the text. Claude sees nothing from it."
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
            Turn on
          </button>
        </div>
      </Row>
    );

  return (
    <Row
      label="Prompt hints"
      hint={
        error ??
        (status === "on"
          ? "On. When a prompt looks quick and you're on Opus in a small chat, the panel suggests Sonnet."
          : status === "broken"
            ? "StatusPal moved since this was turned on. Turn it on again to fix it."
            : "Guesses if a prompt is quick or big, on your Mac, and suggests a cheaper model when it'd do. Costs no tokens.")
      }
    >
      {status === "on" ? (
        <button className="settings-button" onClick={() => set(false)}>
          Turn off
        </button>
      ) : (
        <button
          className="settings-button settings-button--primary"
          onClick={() => setConfirming(true)}
        >
          {status === "broken" ? "Fix" : "Turn on"}
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

/**
 * The helper pack (crates/core/src/adapters/claude/agents.rs): small subagents on cheaper models that Claude
 * hands simple jobs to by itself. Each switch adds or removes one file in ~/.claude/agents.
 */
function Helpers() {
  const [helpers, setHelpers] = useState<HelperInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    invoke<HelperInfo[]>("get_helpers").then(setHelpers);
  }, []);
  if (helpers === null) return null;

  return (
    <>
      <p className="settings-card-note">
        {error ??
          "Small agents on cheaper models. Claude hands them simple jobs by itself, each in its own chat, so yours stays small and your limit lasts longer. Each one is a file in ~/.claude/agents."}
      </p>
      {helpers.map((h) => (
        <Row
          key={h.name}
          label={h.name}
          hint={
            h.state === "yours"
              ? "You have your own agent with this name, so it's left alone."
              : `${h.summary} · ${h.model === "haiku" ? "Haiku" : "Sonnet"}`
          }
        >
          <Switch
            checked={h.state === "on"}
            disabled={h.state === "yours"}
            onChange={(on) =>
              invoke<HelperInfo[]>("set_helper", { name: h.name, on })
                .then((list) => {
                  // The first file in a new ~/.claude/agents folder needs one restart of Claude Code.
                  const first = on && !helpers.some((x) => x.state === "on");
                  setHelpers(list);
                  setError(
                    first
                      ? "Added. Restart Claude Code once so it finds the new helpers folder; after that, changes show up by themselves."
                      : null,
                  );
                })
                .catch((e) => setError(String(e)))
            }
          />
        </Row>
      ))}
    </>
  );
}

/** Gemini plans and their daily requests (crates/core/src/adapters/gemini.rs). */
const GEMINI_PLANS: [GeminiPlan, string][] = [
  ["auto", "Auto"],
  ["personal", "Google account · 1,000/day"],
  ["pro", "Google AI Pro · 1,500/day"],
  ["ultra", "Google AI Ultra · 2,000/day"],
  ["standard", "Code Assist Standard · 1,500/day"],
  ["enterprise", "Code Assist Enterprise · 2,000/day"],
  ["apiFree", "API key, free · 250/day"],
  ["apiPaid", "API key, paid · no daily limit"],
];

const EFFORT_NAMES: Record<string, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
};

/**
 * Codex's default effort and the `codex` profiles (crates/core/src/adapters/codex_config.rs). Both edit
 * ~/.codex/config.toml, keeping everything else in it as it was, with a backup saved before the first change.
 */
function CodexSettings() {
  const [setup, setSetup] = useState<CodexSetup | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const check = () =>
      invoke<CodexSetup | null>("get_codex_setup").then(setSetup);
    check();
    // The Codex app writes this file too, so look again when the window comes forward.
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, []);
  if (setup === null) return null;
  if (!setup.found)
    return (
      <p className="settings-card-note">Codex isn't set up on this Mac.</p>
    );

  const run = (command: string, args: Record<string, unknown> = {}) =>
    invoke<CodexSetup>(command, args)
      .then((s) => {
        setSetup(s);
        setError(null);
      })
      .catch((e) => setError(String(e)));

  return (
    <>
      <Row
        label="Default effort"
        stacked
        hint={
          error ??
          (setup.effortBefore
            ? `Was ${EFFORT_NAMES[setup.effortBefore] ?? setup.effortBefore} before StatusPal changed it. New Codex chats use this; restart the Codex app if it doesn't.`
            : "How hard Codex thinks, in the app and the codex command. Most turns don't need Extra high, and it uses up your limit fastest.")
        }
      >
        <div className="settings-buttons">
          <Choice<string>
            value={setup.effort ?? ""}
            options={Object.entries(EFFORT_NAMES)}
            onChange={(effort) => run("set_codex_effort", { effort })}
          />
          {setup.effortBefore && (
            <button
              className="settings-button"
              onClick={() => run("undo_codex_effort")}
            >
              Undo
            </button>
          )}
        </div>
      </Row>
      <Row
        label="Profiles for the codex command"
        hint={
          setup.profiles === "yours"
            ? "You already have a quick, normal or deep profile, so yours are left alone."
            : "Adds quick (low effort), normal (medium) and deep (extra high). In Terminal: codex --profile quick."
        }
      >
        <Switch
          checked={setup.profiles === "on"}
          disabled={setup.profiles === "yours"}
          onChange={(on) => run("set_codex_profiles", { on })}
        />
      </Row>
    </>
  );
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

function Row(props: {
  label: string;
  hint?: string;
  /** Put the control under the text instead of beside it, for wide controls. */
  stacked?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className={`settings-row${props.stacked ? " settings-row--stacked" : ""}`}
    >
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
