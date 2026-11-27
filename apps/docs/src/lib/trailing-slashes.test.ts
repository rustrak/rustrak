import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixHref } from './trailing-slashes.mjs';

const site = 'https://rustrak.dev';
const href = (h: string) => fixHref(`<a href="${h}">`, site);

test('adds the slash to page links, keeping query and hash', () => {
  assert.equal(href('/blog'), '<a href="/blog/">');
  assert.equal(href('/docs/a#b'), '<a href="/docs/a/#b">');
  assert.equal(href('/x?y=1'), '<a href="/x/?y=1">');
});

test('adds it to absolute links on our own site', () => {
  assert.equal(
    href('https://rustrak.dev/upgrading/0-15'),
    '<a href="https://rustrak.dev/upgrading/0-15/">',
  );
});

test('leaves files, slashed paths and other sites alone', () => {
  for (const h of [
    '/',
    '/blog/',
    '/rss.xml',
    '/x.md',
    '//cdn.example.com/a',
    'https://example.com/a',
    'https://rustrak.dev.evil.com/a',
    '#top',
  ])
    assert.equal(href(h), `<a href="${h}">`);
});
