import { describe, expect, it } from 'vitest';
import { pageTransition } from './page-transition';

const at = (pathname: string, index?: number) => ({
  pathname,
  state: index === undefined ? {} : { __TSR_index: index },
});

describe('pageTransition', () => {
  it('slides forward into a deeper page', () => {
    expect(
      pageTransition(
        at('/projects/1/issues', 1),
        at('/projects/1/issues/abc', 2),
      ),
    ).toEqual(['slide-forward']);
  });

  it('slides back out to a shallower page', () => {
    expect(
      pageTransition(
        at('/projects/1/issues/abc', 2),
        at('/projects/1/issues', 3),
      ),
    ).toEqual(['slide-back']);
  });

  it('slides back on the history back button, whatever the depth', () => {
    expect(
      pageTransition(at('/projects/1/issues', 4), at('/projects/1/logs', 3)),
    ).toEqual(['slide-back']);
  });

  it('cross-fades between pages at the same depth: a tab change', () => {
    expect(
      pageTransition(at('/projects/1/issues', 1), at('/projects/1/logs', 2)),
    ).toEqual(['fade']);
  });

  it('does not animate a change of query string on the same page', () => {
    expect(
      pageTransition(at('/projects/1/issues', 1), at('/projects/1/issues', 2)),
    ).toBe(false);
  });

  it('does not animate the first load', () => {
    expect(pageTransition(undefined, at('/projects'))).toBe(false);
  });
});
