// Makes a test fixture from one of your Codex session logs, keeping only what StatusPal reads: the session
// start, the model and effort, and the token / limit readings. Your messages, Codex's replies, tool calls,
// paths and ids are dropped or replaced. Times (and reset times) are moved to start at 2026-01-01 00:00 UTC.
//
//   node scripts/sanitize-codex-log.mjs <rollout-….jsonl> > fixture.jsonl

import { readFileSync } from "node:fs";

const BASE = Date.parse("2026-01-01T00:00:00Z");
let shift = null; // ms to move every time by, from the first line

const out = [];
for (const raw of readFileSync(process.argv[2], "utf8").split("\n")) {
  let line;
  try {
    line = JSON.parse(raw);
  } catch {
    continue;
  }
  const at = Date.parse(line.timestamp);
  if (Number.isNaN(at)) continue;
  shift ??= BASE - at;
  const timestamp = new Date(at + shift).toISOString();
  const p = line.payload ?? {};

  if (line.type === "session_meta") {
    out.push({
      timestamp,
      type: "session_meta",
      payload: { id: "fixture-session" },
    });
  } else if (line.type === "turn_context") {
    out.push({
      timestamp,
      type: "turn_context",
      payload: { model: p.model, effort: p.effort },
    });
  } else if (line.type === "event_msg" && p.type === "token_count") {
    const primary = p.rate_limits?.primary;
    out.push({
      timestamp,
      type: "event_msg",
      payload: {
        type: "token_count",
        info: p.info && { total_token_usage: p.info.total_token_usage },
        rate_limits: primary && {
          primary: {
            used_percent: primary.used_percent,
            window_minutes: primary.window_minutes,
            resets_at:
              primary.resets_at && primary.resets_at + Math.round(shift / 1000),
          },
        },
      },
    });
  } else {
    // Keep that something happened here (a message, a tool call), not what it was.
    out.push({ timestamp, type: line.type });
  }
}
console.log(out.map((line) => JSON.stringify(line)).join("\n"));
