import type { APIRoute } from 'astro';
import { getReleases } from '../changelog';

/**
 * `/versions.json`: the feed the dashboard's update banner reads
 * (`apps/dashboard/src/shared/api/version-check.ts`). The shape is the one
 * the current site writes; only `url` now points at the release's own page.
 */
export const GET: APIRoute = async ({ site }) => {
  const versions = (await getReleases()).map((release) => ({
    version: release.version.replace(/^v/, ''),
    title: release.title,
    description: release.description,
    date: release.date.toISOString().slice(0, 10),
    url: new URL(`/changelog/${release.anchor}`, site).href,
  }));
  return new Response(
    `${JSON.stringify({ latest: versions[0]?.version ?? null, versions }, null, 2)}\n`,
    { headers: { 'Content-Type': 'application/json' } },
  );
};
