/**
 * The few pieces of a chart the benchmark pages need, drawn as SVG at build
 * time instead of with a charting library in the browser. Shared with the
 * tooltip script, which reads the same points back out of the page.
 */

type Domain = [number, number];

export const linear =
  ([d0, d1]: Domain, [r0, r1]: Domain) =>
  (v: number) =>
    r0 + ((v - d0) / (d1 - d0)) * (r1 - r0);

/** No zero on a log scale: the domain starts above it. */
export const log = ([d0, d1]: Domain, range: Domain) => {
  const scale = linear([Math.log10(d0), Math.log10(d1)], range);
  return (v: number) => scale(Math.log10(v));
};

/**
 * The value at `x` on a line of measured points, straight between two of
 * them, and null outside the line (before its first point, after its last).
 */
export function valueAt(points: [number, number][], x: number): number | null {
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || x < first[0] || x > last[0]) return null;
  const i = points.findIndex(([px]) => px >= x);
  const [x1, y1] = points[i];
  if (x1 === x || i === 0) return y1;
  const [x0, y0] = points[i - 1];
  return Math.round(y0 + ((y1 - y0) * (x - x0)) / (x1 - x0));
}

const round = (v: number) => Math.round(v * 10) / 10;

/** An SVG path through screen points. */
export const path = (points: [number, number][]) =>
  points
    .map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${round(x)} ${round(y)}`)
    .join('');
