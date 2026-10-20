// Mirrors the Rust types in crates/core/src/model.rs. Later, ts-rs will generate these automatically.

export type AgentId = "Claude" | "Codex";

export interface LimitWindow {
  used_pct: number;
  window_minutes: number;
  resets_at: number | null; // Unix seconds
  observed_at: number; // Unix seconds
}

export interface UsageSnapshot {
  agent: AgentId;
  windows: LimitWindow[];
}
