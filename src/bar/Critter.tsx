// The pixel critters that sit on the pill: an orange block for Claude, a blue cloud-bot for Codex, a purple
// star for Gemini.
// Each sprite is an 18×18 grid of letters. Every letter is a colour and a body part, so CSS can animate
// the parts separately (blink the eyes, walk the legs, wave the arms). Moods live in critter.css.
import type { ReactElement } from "react";
import type { Gait } from "../bindings/Gait";
import type { Mood } from "./state";
import type { AgentId } from "./types";

type Part = "body" | "eyes" | "arm-l" | "arm-r" | "legs-a" | "legs-b";

interface Ink {
  color: string;
  part: Part;
}

// Claude: a wide block with two square eyes, stubby side arms and four thin legs.
const CLAUDE_GRID = [
  "..................",
  "..................",
  "..................",
  "...HHHHHHHHHHHH...",
  "...OOOOOOOOOOOD...",
  "...OOKKOOOOKKOD...",
  ".aaOOKKOOOOKKODrr.",
  ".aaOOOOOOOOOOODrr.",
  ".AAOOOOOOOOOOODRR.",
  "...OOOOOOOOOOOD...",
  "...OOOOOOOOOOOD...",
  "...DDDDDDDDDDDD...",
  "....x.y....x.y....",
  "....x.y....x.y....",
  "....x.y....x.y....",
  "....X.Y....X.Y....",
  "..................",
  "..................",
];
// Colours are CSS variables (set in tokens.css), so a palette can recolour the critters.
const CLAUDE_MAIN = "var(--claude-main)";
const CLAUDE_SHADE = "var(--claude-shade)";
const CLAUDE_INK: Record<string, Ink> = {
  H: { color: "var(--claude-light)", part: "body" },
  O: { color: CLAUDE_MAIN, part: "body" },
  D: { color: CLAUDE_SHADE, part: "body" },
  K: { color: "#1c1b1f", part: "eyes" },
  a: { color: CLAUDE_MAIN, part: "arm-l" },
  A: { color: CLAUDE_SHADE, part: "arm-l" },
  r: { color: CLAUDE_MAIN, part: "arm-r" },
  R: { color: CLAUDE_SHADE, part: "arm-r" },
  x: { color: CLAUDE_MAIN, part: "legs-a" },
  X: { color: CLAUDE_SHADE, part: "legs-a" },
  y: { color: CLAUDE_MAIN, part: "legs-b" },
  Y: { color: CLAUDE_SHADE, part: "legs-b" },
};

// Codex: a puffy cloud head with a dark visor and two cyan eyes, on a small body.
const CODEX_GRID = [
  ".....OOO..OOO.....",
  "...OOLLLOOLLLOO...",
  "..OLBBBBBBBBBBBO..",
  ".OLBBBBBBBBBBBBBO.",
  ".OBBSSSSSSSSSSBBO.",
  "OBBBSSESSSSESSBBBO",
  "OBBBSSESSSSESSBBBO",
  "OBBBSSSSSSSSSSBBBO",
  ".OBBSSSSSSSSSSBBO.",
  ".OBBBBBBBBBBBBBDO.",
  "..ODDBBBBBBBBDDO..",
  ".....OBBBBBBO.....",
  "....aOBBBBBBOr....",
  "....aOBcBcBBOr....",
  ".....OBBBBBBO.....",
  ".....ODBBBBDO.....",
  ".....OxxOOyyO.....",
  ".....XXXXYYYY.....",
];
const CODEX_LINE = "var(--codex-line)";
const CODEX_BLUE = "var(--codex-main)";
const CODEX_GLOW = "var(--codex-glow)";
const CODEX_INK: Record<string, Ink> = {
  O: { color: CODEX_LINE, part: "body" },
  L: { color: "var(--codex-light)", part: "body" },
  B: { color: CODEX_BLUE, part: "body" },
  D: { color: "var(--codex-shade)", part: "body" },
  S: { color: "var(--codex-visor)", part: "body" },
  c: { color: CODEX_GLOW, part: "body" },
  E: { color: CODEX_GLOW, part: "eyes" },
  a: { color: CODEX_BLUE, part: "arm-l" },
  r: { color: CODEX_BLUE, part: "arm-r" },
  x: { color: CODEX_BLUE, part: "legs-a" },
  X: { color: CODEX_LINE, part: "legs-a" },
  y: { color: CODEX_BLUE, part: "legs-b" },
  Y: { color: CODEX_LINE, part: "legs-b" },
};

// Gemini: a four-pointed star with two eyes and little legs. Its side points are its arms.
const GEMINI_GRID = [
  "........HH........",
  "........HH........",
  ".......HOOD.......",
  "......HOOOOD......",
  ".....HOOOOOOD.....",
  "....HOOOOOOOOD....",
  "..HHOOKKOOKKOODD..",
  "aaOOOOKKOOKKOOOOrr",
  "..DDOOOOOOOOOODD..",
  "....DOOOOOOOOD....",
  ".....DOOOOOOD.....",
  "......DOOOOD......",
  ".......DOOD.......",
  "......x.DD.y......",
  "......x....y......",
  "......X....Y......",
  "..................",
  "..................",
];
const GEMINI_MAIN = "var(--gemini-main)";
const GEMINI_SHADE = "var(--gemini-shade)";
const GEMINI_INK: Record<string, Ink> = {
  H: { color: "var(--gemini-light)", part: "body" },
  O: { color: GEMINI_MAIN, part: "body" },
  D: { color: GEMINI_SHADE, part: "body" },
  K: { color: "#1c1b1f", part: "eyes" },
  a: { color: GEMINI_MAIN, part: "arm-l" },
  r: { color: GEMINI_MAIN, part: "arm-r" },
  x: { color: GEMINI_MAIN, part: "legs-a" },
  X: { color: GEMINI_SHADE, part: "legs-a" },
  y: { color: GEMINI_MAIN, part: "legs-b" },
  Y: { color: GEMINI_SHADE, part: "legs-b" },
};

/** Codex's `>_` face, shown instead of its eyes while it's working. */
const CODEX_PROMPT: [number, number, number][] = [
  [5, 5, 1],
  [6, 6, 1],
  [5, 7, 1],
  [10, 7, 3],
];

const PARTS: Part[] = ["arm-l", "arm-r", "legs-a", "legs-b", "body", "eyes"];

/** One `<rect>` per run of same-letter pixels in a row, grouped by body part. */
function drawGrid(grid: string[], ink: Record<string, Ink>) {
  const byPart = new Map<Part, ReactElement[]>(PARTS.map((p) => [p, []]));
  grid.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      const ch = row[x];
      let end = x + 1;
      while (end < row.length && row[end] === ch) end++;
      const style = ink[ch];
      if (style) {
        byPart
          .get(style.part)!
          .push(
            <rect
              key={`${x},${y}`}
              x={x}
              y={y}
              width={end - x}
              height={1}
              style={{ fill: style.color }}
            />,
          );
      }
      x = end;
    }
  });
  return PARTS.map((part) => (
    <g key={part} className={`c-${part}`}>
      {byPart.get(part)}
    </g>
  ));
}

const SPRITES: Record<AgentId, ReactElement[]> = {
  Claude: drawGrid(CLAUDE_GRID, CLAUDE_INK),
  Codex: drawGrid(CODEX_GRID, CODEX_INK),
  Gemini: drawGrid(GEMINI_GRID, GEMINI_INK),
};

/**
 * The little extras around the head: "!", "?", "z", sparkles, a sweat drop, speed lines, dizzy stars and the
 * "…" of a critter waiting for you. Colours come from the theme, so they read on light pills too.
 */
function Extras() {
  return (
    <>
      <g className="x-alarm" style={{ fill: "var(--level-out)" }}>
        <rect x={19} y={0} width={1} height={4} />
        <rect x={19} y={5} width={1} height={1} />
      </g>
      <g className="x-confused" style={{ fill: "var(--text-primary)" }}>
        <rect x={18} y={0} width={3} height={1} />
        <rect x={20} y={1} width={1} height={1} />
        <rect x={19} y={2} width={2} height={1} />
        <rect x={19} y={3} width={1} height={1} />
        <rect x={19} y={5} width={1} height={1} />
      </g>
      <g className="x-sleep" style={{ fill: "var(--text-secondary)" }}>
        <rect x={18} y={0} width={4} height={1} />
        <rect x={20} y={1} width={1} height={1} />
        <rect x={19} y={2} width={1} height={1} />
        <rect x={18} y={3} width={4} height={1} />
      </g>
      <g className="x-cheer" style={{ fill: "var(--level-normal)" }}>
        <rect className="spark-1" x={18} y={1} width={3} height={1} />
        <rect className="spark-1" x={19} y={0} width={1} height={3} />
        <rect className="spark-2" x={-2} y={4} width={1} height={1} />
        <rect className="spark-2" x={21} y={6} width={1} height={1} />
      </g>
      <g className="x-speed" style={{ fill: "var(--text-tertiary)" }}>
        <rect x={-2} y={6} width={2} height={1} />
        <rect x={-2} y={9} width={1} height={1} />
        <rect x={-2} y={12} width={2} height={1} />
      </g>
      <g className="x-nervous" style={{ fill: "#7fd0ff" }}>
        <rect x={18} y={3} width={1} height={2} />
      </g>
      {/* Collapsed: three stars over the head, twinkling in turn. */}
      <g className="x-dizzy" style={{ fill: "var(--level-normal)" }}>
        <rect className="star-1" x={4} y={0} width={1} height={1} />
        <rect className="star-2" x={9} y={-1} width={1} height={1} />
        <rect className="star-3" x={14} y={0} width={1} height={1} />
      </g>
      {/* Waiting for you: a "…" by the head, dot by dot. */}
      <g className="x-waiting" style={{ fill: "var(--text-primary)" }}>
        <rect className="dot-1" x={17} y={1} width={1} height={1} />
        <rect className="dot-2" x={19} y={1} width={1} height={1} />
        <rect className="dot-3" x={21} y={1} width={1} height={1} />
      </g>
    </>
  );
}

interface CritterProps {
  agent: AgentId;
  mood: Mood;
  /** How fast the legs go. Sleeping (still) when left out. */
  gait?: Gait;
}

/** An agent's critter, animated for its mood. 48×40 px: the sprite, plus room for extras on the right. */
export default function Critter({
  agent,
  mood,
  gait = "sleeping",
}: CritterProps) {
  return (
    <svg
      className={`critter critter--${agent.toLowerCase()} mood-${mood} gait-${gait}`}
      width="48"
      height="40"
      viewBox="-2 -1 24 20"
      shapeRendering="crispEdges"
      aria-label={`${agent}, ${mood}, ${gait}`}
    >
      <g className="c-sprite">
        {SPRITES[agent]}
        {agent === "Codex" && (
          <g className="c-prompt" style={{ fill: CODEX_GLOW }}>
            {CODEX_PROMPT.map(([x, y, w]) => (
              <rect key={`${x},${y}`} x={x} y={y} width={w} height={1} />
            ))}
          </g>
        )}
      </g>
      <Extras />
    </svg>
  );
}
