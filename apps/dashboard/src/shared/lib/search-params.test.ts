import { describe, expect, it } from 'vitest';
import {
  searchOneOf,
  searchPage,
  searchRedirect,
  searchString,
} from './search-params';

/**
 * Whatever these return becomes the URL, not just the props: the router writes
 * the validated search back to the address bar. So the cases worth pinning are
 * the ones where inventing a value would change an address a reader shared.
 */

describe('searchString', () => {
  it('keeps a non-empty string', () => {
    expect(searchString('open')).toBe('open');
  });

  it('reads absent, empty and non-strings as absent', () => {
    expect(searchString(undefined)).toBeUndefined();
    expect(searchString('')).toBeUndefined();
    expect(searchString(42)).toBeUndefined();
    expect(searchString(['a', 'b'])).toBeUndefined();
  });
});

describe('searchPage', () => {
  it('reads a page number', () => {
    expect(searchPage('3')).toBe(3);
    expect(searchPage(3)).toBe(3);
  });

  // Absent rather than `1`, so a route with no `?page=` keeps an address with
  // no `?page=`. The `?? 1` lives at the call site, as it did in every page.
  it('reads anything that is not a page as absent', () => {
    expect(searchPage(undefined)).toBeUndefined();
    expect(searchPage('')).toBeUndefined();
    expect(searchPage('abc')).toBeUndefined();
    expect(searchPage('0')).toBeUndefined();
    expect(searchPage('-3')).toBeUndefined();
    expect(searchPage(Number.NaN)).toBeUndefined();
  });

  it('takes the whole part of a fractional page', () => {
    expect(searchPage('2.9')).toBe(2);
  });
});

describe('searchOneOf', () => {
  const filters = ['open', 'resolved', 'muted', 'all'] as const;

  it('keeps a value the screen has a control for', () => {
    expect(searchOneOf('resolved', filters)).toBe('resolved');
  });

  // The API ignores a filter it cannot parse and answers unfiltered, so passing
  // one on would leave the URL claiming a filter no control shows as selected.
  it('drops one it does not', () => {
    expect(searchOneOf('exploded', filters)).toBeUndefined();
    expect(searchOneOf(undefined, filters)).toBeUndefined();
    expect(searchOneOf(7, filters)).toBeUndefined();
  });
});

describe('searchRedirect', () => {
  it('keeps a path on this origin, with its query and hash', () => {
    expect(searchRedirect('/projects/3/issues?page=2#top')).toBe(
      '/projects/3/issues?page=2#top',
    );
  });

  // Signing in must not be a way to send someone to another site.
  it('drops anything that leaves the origin', () => {
    expect(searchRedirect('https://evil.example/')).toBeUndefined();
    expect(searchRedirect('//evil.example/path')).toBeUndefined();
    expect(searchRedirect('/\\evil.example')).toBeUndefined();
    expect(searchRedirect('javascript:alert(1)')).toBeUndefined();
    expect(searchRedirect('projects')).toBeUndefined();
    expect(searchRedirect(42)).toBeUndefined();
  });

  // Coming back to the login page after signing in would be a loop.
  it('drops the login page itself', () => {
    expect(searchRedirect('/login')).toBeUndefined();
    expect(searchRedirect('/login?redirect=%2F')).toBeUndefined();
  });
});
