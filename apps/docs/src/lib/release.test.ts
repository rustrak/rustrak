import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  anchorOf,
  byNewestFirst,
  channelOf,
  countChanges,
  kindOf,
  splitSections,
} from './release.ts';

const release = (version: string, date = '2026-01-01', id = version) => ({
  id,
  version,
  date: new Date(date),
});

// The page, versions.json and the dashboard's update banner all take the
// first release as the latest one.
test('newest version first, whatever the dates say', () => {
  const sorted = [
    release('v0.9.0', '2026-03-01'),
    // A patch backported after a later minor carries the later date.
    release('v0.8.3', '2026-04-01'),
    release('v0.10.0', '2026-02-01'),
    release('v0.1.x', '2026-05-01'),
  ].sort(byNewestFirst);
  assert.deepEqual(
    sorted.map((r) => r.version),
    ['v0.10.0', 'v0.9.0', 'v0.8.3', 'v0.1.x'],
  );
});

test('equal versions fall back to date, then id', () => {
  const sorted = [
    release('v0.15.0-rc.1', '2026-01-01', 'a'),
    release('v0.15.0-rc.2', '2026-01-02', 'b'),
  ].sort(byNewestFirst);
  assert.deepEqual(
    sorted.map((r) => r.id),
    ['b', 'a'],
  );
});

test('the anchor is the version, as the current links use it', () => {
  assert.equal(anchorOf('v0.32.0'), 'v0-32-0');
  assert.equal(anchorOf('v0.15.0-rc.2'), 'v0-15-0-rc-2');
  assert.equal(anchorOf(''), 'release');
});

test('a .0 is a minor, the rest patches, unversioned ones early', () => {
  assert.equal(channelOf('v0.32.0'), 'minor');
  assert.equal(channelOf('v0.31.2'), 'patch');
  assert.equal(channelOf('v0.1.x'), 'seed');
});

test('a section heading is read for its intent', () => {
  assert.equal(kindOf('Bug fixes'), 'fixed');
  assert.equal(kindOf('Improvements'), 'improved');
  assert.equal(kindOf('Documentation'), 'documented');
  assert.equal(kindOf('Web Vitals'), 'shipped');
});

test('a body splits on its ## headings, keeping a lead paragraph', () => {
  const sections = splitSections(
    'Opening line.\n\n## Web Vitals\n\n- one\n- two\n\n## Fixes\n\n- three\n',
  );
  assert.deepEqual(
    sections.map((s) => [s.title, s.kind, s.markdown]),
    [
      ['', 'shipped', 'Opening line.'],
      ['Web Vitals', 'shipped', '- one\n- two'],
      ['Fixes', 'fixed', '- three'],
    ],
  );
});

test('a ## inside a code fence is not a heading', () => {
  const sections = splitSections('## Config\n\n```toml\n## comment\n```\n');
  assert.equal(sections.length, 1);
  assert.match(sections[0].markdown, /## comment/);
});

test('changes are list items, or paragraphs when there is no list', () => {
  assert.equal(
    countChanges('<ul><li>a</li><li>b<ul><li>c</li></ul></li></ul>'),
    3,
  );
  assert.equal(countChanges('<p>one</p><p>two</p>'), 2);
  assert.equal(countChanges(''), 0);
});
