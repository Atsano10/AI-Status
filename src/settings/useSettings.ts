// The settings file, shared by every window. `Settings` is generated from crates/core/src/settings.rs; Rust owns
// the file and sends `settings-changed` to every window whenever anything changes it.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { Settings } from "../bindings/Settings";

export type { Behaviour } from "../bindings/Behaviour";
export type { Settings };

/** The current settings, kept up to date. null for the few ms before they load. */
export function useSettings(): Settings | null {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    invoke<Settings>("get_settings").then(setSettings);
    const unlisten = listen<Settings>("settings-changed", (e) =>
      setSettings(e.payload),
    );
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);
  return settings;
}

/** Changes only the fields given. Every window, this one included, hears about it through `settings-changed`. */
export function updateSettings(changes: Partial<Settings>): Promise<Settings> {
  return invoke<Settings>("update_settings", { changes });
}
