// Hotkeys as Tauri writes them ("Control+Alt+Space") and as the Mac shows them (⌃⌥Space).
// Rust checks and registers them (src-tauri/src/hotkey.rs).

export type HotkeyError = "invalid" | "needsModifier" | "reserved" | "taken";

export const DEFAULT_HOTKEY = "Control+Alt+Space";

// Apple's order: ⌃ ⌥ ⇧ ⌘.
const MODIFIERS: [string[], string][] = [
  [["control", "ctrl"], "⌃"],
  [["alt", "option"], "⌥"],
  [["shift"], "⇧"],
  [["command", "cmd", "super"], "⌘"],
];

const KEY_NAMES: Record<string, string> = {
  Space: "Space",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Enter: "↩",
  Backspace: "⌫",
  Delete: "⌦",
  Tab: "⇥",
  Escape: "⎋",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

/** "Control+Alt+KeyK" → "⌃⌥K". */
export function formatHotkey(hotkey: string): string {
  const parts = hotkey.split("+");
  const key = parts.pop() ?? "";
  const mods = parts.map((p) => p.toLowerCase());
  const symbols = MODIFIERS.filter(([names]) =>
    names.some((n) => mods.includes(n)),
  ).map(([, symbol]) => symbol);
  return symbols.join("") + keyName(key);
}

function keyName(code: string): string {
  if (KEY_NAMES[code]) return KEY_NAMES[code];
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  return code; // F5, Home, …
}

/** Keys that only modify. Pressing one alone doesn't finish recording. */
const MODIFIER_CODES =
  /^(Shift|Control|Alt|Meta|OS)(Left|Right)?$|^CapsLock$|^Fn$/;

/**
 * The hotkey for a key press, in Tauri's format. `null` while only modifiers are held. Uses `code` (the
 * physical key), so ⌥K is "Alt+KeyK" and not the "˚" that ⌥K types.
 */
export function hotkeyFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.test(e.code)) return null;
  const mods = [
    e.ctrlKey && "Control",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Command",
  ].filter(Boolean);
  return [...mods, e.code].join("+");
}

/** The modifiers held right now, e.g. "⌃⌥", shown while recording. */
export function heldModifiers(e: KeyboardEvent): string {
  return [
    e.ctrlKey && "⌃",
    e.altKey && "⌥",
    e.shiftKey && "⇧",
    e.metaKey && "⌘",
  ]
    .filter(Boolean)
    .join("");
}

export function hotkeyErrorText(error: HotkeyError): string {
  switch (error) {
    case "needsModifier":
      return "Add ⌘, ⌥ or ⌃. On its own, that key would stop typing in every app.";
    case "reserved":
      return "macOS already uses that one. Try another.";
    case "taken":
      return "Another app already uses that one. Try another.";
    case "invalid":
      return "That key can't be a shortcut. Try another.";
  }
}
