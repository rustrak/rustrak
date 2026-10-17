/**
 * Real sponsors from GitHub Sponsors and Open Collective, read at build time.
 *
 * The Sponsors data is not in the REST API and the GraphQL one needs a token,
 * which a static site has nowhere safe to keep. `sponsors_partial` is the
 * fragment GitHub's own sponsors page loads its avatar wall from: public, no
 * auth, and it returns exactly the people who are actually sponsoring right
 * now. The trade is that it is HTML rather than JSON, so everything here is
 * written to fail closed — an unrecognised response yields no sponsors, and no
 * sponsors means the section does not render at all.
 *
 * `apps/docs` is `output: 'export'`, so this runs during `next build`. The
 * wall therefore refreshes on deploy, not on visit. Fetching it in the browser
 * instead is not an option: github.com sends no CORS headers for this path.
 */

/**
 * How prominently a sponsor is shown, which is what the Open Collective tiers
 * promise: Gold Sponsors get the largest cards, Sponsors the regular ones,
 * Supporters and Backers a name in the list below.
 */
export type SponsorLevel = 'gold' | 'sponsor' | 'backer';

export interface Sponsor {
  name: string;
  url: string;
  avatar: string;
  isOrganization: boolean;
  level: SponsorLevel;
}

/** Tier names exactly as they are set up on the Open Collective dashboard. */
const LEVEL_BY_TIER: Record<string, SponsorLevel> = {
  'Gold Sponsor': 'gold',
  Sponsor: 'sponsor',
};

const SPONSORS_URL =
  'https://github.com/sponsors/AbianS/sponsors_partial?filter=all';

/**
 * Open Collective publishes its members as plain JSON, no auth. Admins, the
 * host and followers are members too, so only active `BACKER`s are kept: the
 * people with a running contribution, whatever tier it is in.
 */
const OPEN_COLLECTIVE_URL =
  'https://opencollective.com/rustrak/members/all.json';

interface OpenCollectiveMember {
  role: string;
  isActive: boolean;
  type: string;
  name: string;
  profile: string;
  image: string | null;
  totalAmountDonated: number;
  /** Absent for one-off and custom donations, which belong to no tier. */
  tier?: string;
}

/**
 * One anchor per sponsor: the hovercard type tells us whether it is a person
 * or an org, the href carries the login and the nested img the avatar.
 */
const ENTRY =
  /<a[^>]*data-hovercard-type="(user|organization)"[^>]*href="\/([^"/]+)"[^>]*>\s*<img[^>]*src="([^"]+)"/g;

/**
 * Attribute values arrive HTML-escaped, so an avatar URL reads
 * `?s=60&amp;v=4`. Handed straight to `src` that becomes a query parameter
 * literally named `amp;v`, which happens to work today only because the
 * parameter it mangles is optional.
 */
function decode(value: string) {
  return value
    .replace(/&amp;/g, '&')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

/**
 * Every sponsor from both places, Open Collective first and ordered by how
 * much each has given, so within a level the bigger supporters lead (which
 * also puts Supporters ahead of Backers). Each source fails
 * closed on its own: one being down never empties the other.
 */
export async function fetchSponsors(): Promise<Sponsor[]> {
  const [openCollective, github] = await Promise.all([
    fetchOpenCollectiveSponsors(),
    fetchGitHubSponsors(),
  ]);
  return [...openCollective, ...github];
}

async function fetchOpenCollectiveSponsors(): Promise<Sponsor[]> {
  try {
    const response = await fetch(OPEN_COLLECTIVE_URL, {
      signal: AbortSignal.timeout(8000),
      cache: 'force-cache',
    });
    if (!response.ok) return [];

    const members: OpenCollectiveMember[] = await response.json();
    if (!Array.isArray(members)) return [];

    return members
      .filter((member) => member.role === 'BACKER' && member.isActive)
      .sort((a, b) => b.totalAmountDonated - a.totalAmountDonated)
      .map((member) => ({
        name: member.name,
        url: member.profile,
        // Members without an uploaded image still have a generated one here.
        avatar:
          member.image ??
          `https://images.opencollective.com/${member.profile.split('/').pop()}/avatar.png`,
        isOrganization: member.type !== 'USER',
        level: LEVEL_BY_TIER[member.tier ?? ''] ?? 'backer',
      }));
  } catch {
    return [];
  }
}

async function fetchGitHubSponsors(): Promise<Sponsor[]> {
  try {
    const response = await fetch(SPONSORS_URL, {
      headers: { Accept: 'text/html' },
      // A slow or unreachable GitHub must never hold up a build.
      signal: AbortSignal.timeout(8000),
      cache: 'force-cache',
    });
    if (!response.ok) return [];

    const html = await response.text();
    const sponsors: Sponsor[] = [];
    const seen = new Set<string>();

    for (const [, type, login, avatar] of html.matchAll(ENTRY)) {
      if (!login || !avatar || seen.has(login)) continue;
      seen.add(login);
      sponsors.push({
        name: login,
        url: `https://github.com/${login}`,
        // The partial asks for 30px; ask for one that survives a retina card.
        avatar: decode(avatar).replace(/([?&]s=)\d+/, '$1160'),
        isOrganization: type === 'organization',
        // The partial carries no amounts, so GitHub sponsors keep the regular
        // card they always had.
        level: 'sponsor',
      });
    }

    return sponsors;
  } catch {
    // Offline builds, a rename, a markup change: all of them mean "no wall".
    return [];
  }
}
