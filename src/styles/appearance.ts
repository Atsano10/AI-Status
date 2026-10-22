// Applies Settings ▸ Look to the page: data attributes on the root element that themes.css reacts to.
// Every window renders <Appearance /> once, so a change in Settings shows everywhere straight away.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { useSettings, type Settings } from "../settings/useSettings";

/** Whether macOS is in dark mode right now, following changes. */
function useSystemDark(): boolean {
  const query = "(prefers-color-scheme: dark)";
  const [dark, setDark] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const change = () => setDark(media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  return dark;
}

/** Sets the look on `root`. The dev gallery calls this directly with its own choices. */
export function applyLook(
  root: HTMLElement,
  look: Pick<Settings, "palette" | "surface" | "buttons" | "cards">,
  theme: "dark" | "light",
) {
  root.dataset.palette = look.palette;
  root.dataset.theme = theme;
  root.dataset.surface = look.surface;
  root.dataset.buttons = look.buttons;
  root.dataset.cards = look.cards;
}

/**
 * Keeps this window's look in step with the settings. `titleBar`: also switch the window's title bar
 * between dark and light (the settings window has one; the bar doesn't).
 */
export default function Appearance({
  titleBar = false,
}: {
  titleBar?: boolean;
}) {
  const settings = useSettings();
  const systemDark = useSystemDark();
  useEffect(() => {
    if (!settings) return;
    const theme =
      settings.theme === "system"
        ? systemDark
          ? "dark"
          : "light"
        : settings.theme;
    applyLook(document.documentElement, settings, theme);
    if (titleBar) {
      // null follows macOS.
      void getCurrentWindow().setTheme(
        settings.theme === "system" ? null : settings.theme,
      );
    }
  }, [settings, systemDark, titleBar]);
  return null;
}
