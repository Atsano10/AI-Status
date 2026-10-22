# StatusPal

A little floating bar for macOS that shows how much of your AI coding limits you've used, with a pixel critter
that walks, jogs, and runs as you burn through tokens.

It reads the usage data that Claude Code, Codex, and Gemini CLI already save on your Mac, so there's nothing to
sign in to and nothing leaves your computer.

## What it does

- **A bar that floats over everything.** Press `⌃⌥Space` (you can change it) to show or hide it. It shows the
  limit for the agent you're using right now, and you can drag it anywhere.
- **A panel with the details.** Click the bar to see every limit window, when it resets, and your other agents.
- **A critter with moods.** It sleeps when you're idle, picks up the pace as you use more tokens, gets nervous
  near the limit, collapses when you run out, cheers when the limit resets, and looks at you when the agent is
  done and waiting for you.
- **Notifications** when a limit reaches 80%, runs out, or resets.
- **Tips** based on how you're using it, like when you're on pace to run out or a reply keeps re-sending a huge
  context. Each one has a Copy button and can be hidden.
- **Prompt hints for Claude Code.** An optional hook that suggests a lighter or heavier model for the prompt you
  just sent.
- **Your look.** 5 colour palettes, dark or light, solid or see-through, rounded or pixel corners, and a
  horizontal or vertical bar (on a vertical bar, the critter climbs).

## What it tracks

| Agent                                    | Where the numbers come from                                                                                     | Limits shown                                                      |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| **Claude** (Claude Code, Claude Desktop) | Claude Code's status line, which StatusPal connects to in Settings. Claude Desktop's usage history is a backup. | 5-hour and weekly                                                 |
| **Codex** (CLI and desktop app)          | The session logs in `~/.codex/sessions`                                                                         | Whatever your ChatGPT plan has, e.g. 5-hour and weekly, or 30-day |
| **Gemini CLI**                           | The chat logs in `~/.gemini`, counted against your plan's daily allowance                                       | Daily (resets at midnight Pacific)                                |

Chats in a browser (claude.ai, ChatGPT, gemini.google.com) don't save anything on your Mac, so they can't be
tracked. Use on the same account still shows up the next time one of the tools above checks in.

## Install

1. Download the `.dmg` from the latest release on the [Releases page](https://github.com/Atsano10/StatusPal/releases).
2. Open it and drag **StatusPal** into **Applications**.
3. Open StatusPal. macOS will say it can't check the app for malware, because it isn't signed with a paid
   Apple developer account. To open it anyway:
   - Click **Done**, then go to **System Settings ▸ Privacy & Security**, scroll down, and click **Open
     Anyway** next to StatusPal.
   - Or run this once in Terminal: `xattr -dr com.apple.quarantine /Applications/StatusPal.app`
4. StatusPal lives in the menu bar (🐾), not the Dock. Open **Settings…** from there and:
   - **Connect Claude Code**, so it can read your Claude limits. If you move the app later, connect it again.
   - Turn on anything else you want: prompt hints, Claude Code helpers, open at login.

StatusPal needs macOS on Apple Silicon or Intel. Windows and Linux aren't supported yet.

## Privacy

- **Everything stays on your Mac.** StatusPal makes no network requests and has no analytics.
- **It only reads usage numbers.** The logs it reads also contain your prompts and code, but StatusPal's parsers
  only pick out token counts, model names, and limit percentages. Everything else is skipped.
- It never uses your Claude, ChatGPT, or Google login.
- Its own settings are saved in `~/Library/Application Support/com.atsano10.statuspal/`.

## Known limitations

- **Claude's numbers are rounded.** Claude Code reports whole percentages and resets on the hour, so StatusPal
  can read about 1% lower than claude.ai and show the reset a few minutes early. That's why Claude's numbers have
  a `~`.
- **A shared account lags behind.** StatusPal only gets a new Claude or Codex reading when that tool runs on
  this Mac. Use from other devices, or other people on your account, shows up after your next reply here. The
  panel says when a reading is more than 15 minutes old.
- **Gemini is an estimate.** Gemini CLI doesn't save a percentage, so StatusPal counts today's requests against
  your plan's allowance. Pick your plan in Settings ▸ Gemini.
- **The log formats aren't official.** Codex and Gemini CLI can change how they save logs in any update, which
  could break their numbers until StatusPal catches up.

## Building from source

You'll need [Node.js](https://nodejs.org) 22+, [Rust](https://rustup.rs), and Xcode's command line tools
(`xcode-select --install`).

```sh
npm install
npm run tauri dev     # run the app, rebuilding when files change
npm run tauri build   # build StatusPal.app and a .dmg into target/release/bundle/
```

While `tauri dev` is running, `http://localhost:1420/gallery.html` shows every bar and panel state at once.

The checks CI runs:

```sh
npm run lint && npm run format:check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### How the code is laid out

- `crates/core`: all the logic, with no UI. It reads each agent's data (`adapters/`), keeps the usage numbers
  (`tracker.rs`), and decides the critter's gait, the tips, and the notifications. Test it with
  `cargo test -p statuspal-core`.
- `src-tauri`: the macOS app around it: the floating panel, the tray menu, the hotkey, file watching, and the
  settings file.
- `src`: the React frontend for the bar, the panel, and the settings window. `src/bindings` is generated from
  the Rust types by `cargo test`, so don't edit it by hand.

## License

[MIT](LICENSE). StatusPal isn't affiliated with Anthropic, OpenAI, or Google.
