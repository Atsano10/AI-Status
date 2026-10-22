import type { ReactNode } from "react";
import type { Orientation, PreferredWindow } from "../bar/state";
import type { AgentId } from "../bar/types";
import ShortcutRecorder from "./ShortcutRecorder";
import { updateSettings, useSettings } from "./useSettings";

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
    </main>
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
