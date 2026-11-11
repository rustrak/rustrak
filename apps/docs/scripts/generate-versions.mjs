// Writes public/versions.json, the feed the dashboard's update banner reads,
// from the same parse and order as the changelog page.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { getReleases } from '../src/lib/changelog.ts';
import { SITE } from '../src/lib/seo.ts';

const OUTPUT = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '../public/versions.json',
);

const versions = getReleases()
  .filter((release) => release.version !== '')
  .map((release) => ({
    version: release.version.replace(/^v/, ''),
    title: release.title,
    description: release.description,
    date: release.date,
    url: `${SITE}/changelog#${release.anchor}`,
  }));
const payload = { latest: versions[0]?.version ?? null, versions };

fs.mkdirSync(path.dirname(OUTPUT), { recursive: true });
fs.writeFileSync(OUTPUT, `${JSON.stringify(payload, null, 2)}\n`);

console.log(
  `versions.json: ${versions.length} releases, latest ${payload.latest}`,
);
