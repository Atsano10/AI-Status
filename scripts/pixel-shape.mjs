// Prints a CSS `polygon()` for a box with stepped, pixel-art corners: each corner is a quarter circle of
// `radius` px drawn in `step` px blocks, like the critters. Paste the output into src/styles/tokens.css.
//
//   node scripts/pixel-shape.mjs <radius> <step>      e.g. node scripts/pixel-shape.mjs 24 4

const [radius = 24, step = 4] = process.argv.slice(2).map(Number);
const rows = Math.round(radius / step);

// The top-left corner as [x, y] points, walking down from the top edge. Each row of blocks starts a bit
// further left, following the circle.
const corner = [];
for (let row = 0; row < rows; row++) {
  const fromCentre = radius - (row + 0.5) * step;
  const x = radius - Math.sqrt(Math.max(0, radius ** 2 - fromCentre ** 2));
  const inset = Math.round(x / step) * step;
  corner.push([inset, row * step], [inset, (row + 1) * step]);
}

// [x, y] plus which sides they're measured from (true = from the right / bottom).
const flip = (points, right, bottom) =>
  points.map(([x, y]) => [x, y, right, bottom]);
const path = [
  ...flip([...corner].reverse(), false, false), // top-left, going up the left side to the top
  ...flip(corner, true, false), // top-right, going down
  ...flip([...corner].reverse(), true, true), // bottom-right
  ...flip(corner, false, true), // bottom-left, going up
];

const css = (n, fromEnd) =>
  fromEnd
    ? n === 0
      ? "100%"
      : `calc(100% - ${n}px)`
    : n === 0
      ? "0"
      : `${n}px`;
const points = path
  .map(([x, y, right, bottom]) => `${css(x, right)} ${css(y, bottom)}`)
  .filter((p, i, list) => p !== list[i - 1]); // drop repeats where a row didn't step
console.log(`polygon(${points.join(", ")})`);
