import assert from 'node:assert/strict';
import { test } from 'node:test';
import { getReleases } from './changelog.ts';
import { parseVersion } from './release.ts';

// The page, versions.json and the dashboard's update banner all take the
// first release as the latest one.
test('releases are newest version first, unparseable ones last', () => {
  const releases = getReleases();
  assert.ok(releases.length > 0);

  const versions = releases.map((release) => parseVersion(release.version));
  const firstUnparsed = versions.indexOf(null);
  const parsed = (
    firstUnparsed === -1 ? versions : versions.slice(0, firstUnparsed)
  ) as [number, number, number][];
  assert.ok(versions.slice(parsed.length).every((v) => v === null));

  for (let i = 1; i < parsed.length; i += 1) {
    const [a, b] = [parsed[i - 1], parsed[i]];
    const order = a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
    assert.ok(
      order >= 0,
      `${releases[i - 1].version} before ${releases[i].version}`,
    );
  }
});

// The anchor is the version: two files for one version would share a link.
test('every release has its own anchor', () => {
  const releases = getReleases();
  assert.equal(new Set(releases.map((r) => r.anchor)).size, releases.length);
});
