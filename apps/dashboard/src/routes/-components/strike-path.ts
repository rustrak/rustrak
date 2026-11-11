export interface Point {
  x: number;
  y: number;
}

/** A point on a curve, with the distance along it to there. */
export interface CurvePoint extends Point {
  s: number;
}

const SAMPLES = 24;

/** Where the line from `from` to `center` crosses a box around `center`. */
export function rectEdge(
  from: Point,
  center: Point,
  halfWidth: number,
  halfHeight: number,
): Point {
  const dx = from.x - center.x;
  const dy = from.y - center.y;
  const scale =
    1 / Math.max(Math.abs(dx) / halfWidth, Math.abs(dy) / halfHeight);
  return { x: center.x + dx * scale, y: center.y + dy * scale };
}

/** A quadratic curve as points, for measuring it and testing it for hits. */
export function sampleCurve(from: Point, control: Point, to: Point) {
  const points: CurvePoint[] = [{ ...from, s: 0 }];
  for (let i = 1; i <= SAMPLES; i++) {
    const t = i / SAMPLES;
    const u = 1 - t;
    const x = u * u * from.x + 2 * u * t * control.x + t * t * to.x;
    const y = u * u * from.y + 2 * u * t * control.y + t * t * to.y;
    const previous = points[i - 1];
    points.push({
      x,
      y,
      s: previous.s + Math.hypot(x - previous.x, y - previous.y),
    });
  }
  return points;
}

/** Whether every point of a curve stays `radius` away from every obstacle. */
export function clearOf(points: Point[], obstacles: Point[], radius: number) {
  return points.every((point) =>
    obstacles.every(
      (obstacle) =>
        Math.hypot(point.x - obstacle.x, point.y - obstacle.y) >= radius,
    ),
  );
}
