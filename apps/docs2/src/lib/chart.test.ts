import assert from 'node:assert/strict';
import { test } from 'node:test';
import { linear, log, path, valueAt } from './chart.ts';

test('a linear scale maps its domain onto its range', () => {
  const x = linear([-20, 60], [0, 800]);
  assert.equal(x(-20), 0);
  assert.equal(x(60), 800);
  assert.equal(x(20), 400);
});

test('a log scale puts each decade the same distance apart', () => {
  const x = log([1, 100], [0, 200]);
  assert.equal(x(1), 0);
  assert.equal(x(10), 100);
  assert.equal(x(100), 200);
});

test('a value between two counts is the straight line between them', () => {
  const points: [number, number][] = [
    [0, 0],
    [4, 400],
    [6, 1000],
  ];
  assert.equal(valueAt(points, 2), 200);
  assert.equal(valueAt(points, 5), 700);
  assert.equal(valueAt(points, 6), 1000);
  assert.equal(valueAt(points, 7), null);
  assert.equal(valueAt(points, -1), null);
});

test('a path joins its points, rounded to a tenth of a pixel', () => {
  assert.equal(
    path([
      [0, 10],
      [5.04, 2.96],
    ]),
    'M0 10L5 3',
  );
});
