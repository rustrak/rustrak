/**
 * The roadmap, read from the public GitHub project rather than written here.
 *
 * The project is public, and GitHub's REST API for Projects answers for a
 * public project with no token and with CORS open, so the same function runs
 * at build time (the HTML ships with the board already drawn) and in the
 * browser (the board is refreshed with what the project says right now).
 * Unauthenticated, that is 60 requests an hour per visitor's IP, and a visit
 * costs two. `GITHUB_TOKEN`, when the build has one, only lifts the build's
 * own limit; it is never in the browser bundle.
 */

export const PROJECT_URL = 'https://github.com/orgs/rustrak/projects/2';
const API = 'https://api.github.com/orgs/rustrak/projectsV2/2';

export const HORIZONS = ['Now', 'Next', 'Later'] as const;
export type Horizon = (typeof HORIZONS)[number];

export type RoadmapItem = {
  id: number;
  title: string;
  number: number | null;
  url: string;
  /** The project's Status: Exploring, In design, In progress, In review, Shipped. */
  status: string | null;
  horizon: Horizon;
  area: string | null;
  closedAt: string | null;
};

/** The project's `Primary area` values, as the site names them. */
const AREAS: Record<string, string> = {
  server: 'Server',
  'webview-ui': 'Dashboard',
  client: 'Client',
  mcp: 'MCP',
  docs: 'Docs',
  infra: 'Infra',
};

type RawField = { name: string; value: unknown };
type RawItem = {
  id: number;
  content?: {
    title?: string;
    number?: number;
    html_url?: string;
    closed_at?: string | null;
  } | null;
  fields?: RawField[];
};

/** `feat(sourcemaps): pluggable…` reads as `Pluggable…` on a roadmap. */
export function cleanTitle(title: string): string {
  const bare = title.replace(/^[a-z]+(\([^)]*\))?!?:\s*/i, '');
  return bare.charAt(0).toUpperCase() + bare.slice(1);
}

/** A single-select's option name, or null when the field is unset. */
function option(fields: RawField[], name: string): string | null {
  const value = fields.find((field) => field.name === name)?.value as
    | { name?: { raw?: string } }
    | null
    | undefined;
  return value?.name?.raw ?? null;
}

export function toItem(raw: RawItem): RoadmapItem {
  const fields = raw.fields ?? [];
  const horizon = option(fields, 'Horizon');
  const area = option(fields, 'Primary area');
  return {
    id: raw.id,
    title: cleanTitle(raw.content?.title ?? 'Untitled'),
    number: raw.content?.number ?? null,
    url: raw.content?.html_url ?? PROJECT_URL,
    status: option(fields, 'Status'),
    // An item nobody has placed yet is not a promise: it goes furthest out.
    horizon: HORIZONS.find((h) => h === horizon) ?? 'Later',
    area: area ? (AREAS[area] ?? area) : null,
    closedAt: raw.content?.closed_at ?? null,
  };
}

/** Every item on the project. Throws on any failed request. */
export async function fetchRoadmap(): Promise<RoadmapItem[]> {
  const token =
    typeof window === 'undefined' ? process.env.GITHUB_TOKEN : undefined;
  const headers: Record<string, string> = {
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
  const get = async (url: string) => {
    const response = await fetch(url, { headers });
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return response;
  };

  // Field ids are asked for rather than written down, so recreating a field
  // on the project does not silently empty the board.
  const fields: { id: number; name: string }[] = await (
    await get(`${API}/fields`)
  ).json();
  const ids = fields
    .filter((f) => ['Status', 'Horizon', 'Primary area'].includes(f.name))
    .map((f) => f.id)
    .join(',');

  const items: RawItem[] = [];
  let url: string | null = `${API}/items?per_page=100&fields=${ids}`;
  while (url) {
    const response = await get(url);
    items.push(...(await response.json()));
    url =
      response.headers.get('link')?.match(/<([^>]+)>;\s*rel="next"/)?.[1] ??
      null;
  }
  return items.map(toItem);
}
