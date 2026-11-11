import { describe, expect, it } from 'vitest';
import { clearOf, rectEdge, sampleCurve } from './strike-path';

describe('rectEdge', () => {
  const center = { x: 400, y: 300 };

  it('stops on the side of the box the line comes through', () => {
    expect(rectEdge({ x: 800, y: 300 }, center, 50, 13)).toEqual({
      x: 450,
      y: 300,
    });
    expect(rectEdge({ x: 400, y: 0 }, center, 50, 13)).toEqual({
      x: 400,
      y: 287,
    });
  });

  it('never lands outside the box', () => {
    const { x, y } = rectEdge({ x: 100, y: 650 }, center, 50, 13);
    expect(Math.abs(x - 400)).toBeLessThanOrEqual(50);
    expect(Math.abs(y - 300)).toBeCloseTo(13);
  });
});

describe('sampleCurve', () => {
  it('runs from its start to its end, measuring distance as it goes', () => {
    const points = sampleCurve(
      { x: 0, y: 0 },
      { x: 50, y: 0 },
      { x: 100, y: 0 },
    );
    expect(points[0]).toEqual({ x: 0, y: 0, s: 0 });
    expect(points.at(-1)?.x).toBeCloseTo(100);
    expect(points.at(-1)?.s).toBeCloseTo(100);
  });
});

describe('clearOf', () => {
  const line = sampleCurve({ x: 0, y: 0 }, { x: 50, y: 0 }, { x: 100, y: 0 });

  it('refuses a path that passes within the radius of an obstacle', () => {
    expect(clearOf(line, [{ x: 50, y: 10 }], 20)).toBe(false);
  });

  it('accepts a path that keeps its distance', () => {
    expect(clearOf(line, [{ x: 50, y: 40 }], 20)).toBe(true);
    expect(clearOf(line, [], 20)).toBe(true);
  });
});
