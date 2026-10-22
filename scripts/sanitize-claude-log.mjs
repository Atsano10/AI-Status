// Makes a test fixture from one of your Claude Code chat logs, keeping only what StatusPal reads: reply ids,
// models, effort, token counts and the time between lines. Your messages, Claude's replies, file paths and
// ids are all dropped or replaced. Times are moved to start at 2026-01-01 00:00 UTC.
//
//   node scripts/sanitize-claude-log.mjs <log.jsonl> <from ISO time> <minutes> > fixture.jsonl

import { readFileSync } from "node:fs";

const [path, from, minutes = "60"] = process.argv.slice(2);
const start = Date.parse(from);
const end = start + Number(minutes) * 60_000;
const BASE = Date.parse("2026-01-01T00:00:00Z");

const ids = new Map(); // real message id → msg_0001, msg_0002, …
const out = [];
for (const raw of readFileSync(path, "utf8").split("\n")) {
  let line;
  try {
    line = JSON.parse(raw);
  } catch {
    continue;
  }
  const at = Date.parse(line.timestamp);
  if (!(at >= start && at < end)) continue;
  const timestamp = new Date(BASE + (at - start)).toISOString();

  const usage = line.message?.usage;
  if (line.type !== "assistant" || !usage) {
    // Keep that something else happened here (your message, a tool result), not what it was.
    out.push({ type: line.type, timestamp });
    continue;
  }
  if (!ids.has(line.message.id)) {
    ids.set(line.message.id, `msg_${String(ids.size + 1).padStart(4, "0")}`);
  }
  out.push({
    type: "assistant",
    timestamp,
    sessionId: "fixture-session",
    effort: line.effort,
    message: {
      id: ids.get(line.message.id),
      model: line.message.model,
      usage: {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        output_tokens_details: usage.output_tokens_details,
      },
    },
  });
}
console.log(out.map((line) => JSON.stringify(line)).join("\n"));
