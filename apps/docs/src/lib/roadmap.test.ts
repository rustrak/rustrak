import assert from 'node:assert/strict';
import { test } from 'node:test';
import { cleanTitle, toItem } from './roadmap.ts';

/** One item as the REST API returns it, trimmed to what is read. */
const raw = {
  id: 1,
  content_type: 'Issue',
  content: {
    title: 'feat(sourcemaps): pluggable storage backends',
    number: 42,
    html_url: 'https://github.com/rustrak/rustrak/issues/42',
    closed_at: null,
  },
  fields: [
    { name: 'Title', value: { raw: 'ignored' } },
    { name: 'Status', value: { name: { raw: 'In progress' } } },
    { name: 'Horizon', value: { name: { raw: 'Now' } } },
    { name: 'Primary area', value: { name: { raw: 'webview-ui' } } },
  ],
};

test('reads status, horizon and area from the project fields', () => {
  assert.deepEqual(toItem(raw), {
    id: 1,
    title: 'Pluggable storage backends',
    number: 42,
    url: 'https://github.com/rustrak/rustrak/issues/42',
    status: 'In progress',
    horizon: 'Now',
    area: 'Dashboard',
    closedAt: null,
  });
});

test('an item with no horizon is Later, and unset fields are null', () => {
  const item = toItem({ ...raw, fields: [] });
  assert.equal(item.horizon, 'Later');
  assert.equal(item.status, null);
  assert.equal(item.area, null);
});

test('conventional prefixes come off the title', () => {
  assert.equal(
    cleanTitle('fix: make sign-in case-insensitive'),
    'Make sign-in case-insensitive',
  );
  assert.equal(
    cleanTitle('RFC: Evaluate a storage server'),
    'Evaluate a storage server',
  );
  assert.equal(cleanTitle('Sentry sync'), 'Sentry sync');
  // Only the known prefixes are dropped, not any word before a colon.
  assert.equal(cleanTitle('API: rate limits'), 'API: rate limits');
});
