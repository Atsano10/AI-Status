import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import {
  DEFAULT_HOTKEY,
  formatHotkey,
  heldModifiers,
  hotkeyErrorText,
  hotkeyFromEvent,
  type HotkeyError,
} from "./hotkey";

/**
 * "Record shortcut": click it, press a combo, done. Esc or clicking away cancels. While it listens, the current
 * hotkey is switched off (`pause_hotkey`) so pressing it reaches us instead of toggling the bar.
 */
export default function ShortcutRecorder({ hotkey }: { hotkey: string }) {
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState("");
  const [error, setError] = useState<HotkeyError | null>(null);
  // Your hotkey might also be a macOS shortcut (picked before StatusPal checked, or macOS changed since).
  const [clash, setClash] = useState<string | null>(null);
  useEffect(() => {
    invoke<string | null>("get_hotkey_warning").then(setClash);
  }, [hotkey]);

  useEffect(() => {
    if (!recording) return;
    void invoke("pause_hotkey", { paused: true });

    const stop = () => {
      setRecording(false);
      setHeld("");
    };
    const cancel = () => {
      stop();
      void invoke("pause_hotkey", { paused: false });
    };
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.type === "keyup") {
        setHeld(heldModifiers(e));
        return;
      }
      if (e.code === "Escape" && heldModifiers(e) === "") return cancel();
      const next = hotkeyFromEvent(e);
      if (next === null) {
        setHeld(heldModifiers(e)); // only modifiers so far
        return;
      }
      stop();
      // Rust switches the hotkey over and saves it; settings-changed then updates every window.
      invoke("set_hotkey", { hotkey: next })
        .then(() => setError(null))
        .catch((err: HotkeyError) => {
          setError(err);
          void invoke("pause_hotkey", { paused: false }); // the old one comes back
        });
    };

    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
      window.removeEventListener("blur", cancel);
    };
  }, [recording]);

  return (
    <div className="recorder">
      <div className="recorder-controls">
        {hotkey !== DEFAULT_HOTKEY && !recording && (
          <button
            className="recorder-reset"
            onClick={() =>
              invoke("set_hotkey", { hotkey: DEFAULT_HOTKEY })
                .then(() => setError(null))
                .catch((err: HotkeyError) => setError(err))
            }
          >
            Reset
          </button>
        )}
        <button
          className={`recorder-key${recording ? " is-recording" : ""}`}
          onClick={() => {
            setError(null);
            setRecording((r) => !r);
            if (recording) void invoke("pause_hotkey", { paused: false });
          }}
        >
          {recording ? held || "Type shortcut…" : formatHotkey(hotkey)}
        </button>
      </div>
      {error && (
        <span className="recorder-error">{hotkeyErrorText(error)}</span>
      )}
      {!error && clash && (
        <span className="recorder-error">
          macOS also uses this for “{clash}”. Pick another, or turn that off in
          System Settings ▸ Keyboard ▸ Keyboard Shortcuts.
        </span>
      )}
    </div>
  );
}
