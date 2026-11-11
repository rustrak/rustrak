const normalize = (value: string) =>
  value
    .toLowerCase()
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '');

/** Drops every separator, so "api-token" and "API Tokens" meet as one word. */
const squash = (value: string) => value.replace(/[^a-z0-9]+/g, '');

/** The forms of one row's text a term is tried against, in scoring order. */
interface Forms {
  target: string;
  words: string[];
  acronym: string;
  squashed: string;
}

function scoreTerm(
  { target, words, acronym, squashed }: Forms,
  term: string,
  squashedTerm: string,
): number {
  // A term short enough to be a stray keystroke is not allowed to cross a word
  // boundary: at one character `squashed` would match nearly every row, and at
  // two it matches on the seam between words, "ea" pulling in "Acme API".
  const canCrossWords = squashedTerm.length > 2;

  if (target.startsWith(term)) return 1;
  if (words.some((word) => word.startsWith(term))) return 0.8;
  if (term.length > 1 && acronym.startsWith(term)) return 0.6;
  if (canCrossWords && squashed.startsWith(squashedTerm)) return 0.5;
  if (target.includes(term)) return 0.3;
  if (canCrossWords && squashed.includes(squashedTerm)) return 0.2;
  return 0;
}

/**
 * Ranks one command row against a query. Returns 0 when the row should be
 * hidden, and a larger number the better it matches.
 *
 * Deliberately stricter than the fuzzy subsequence matching cmdk ships with.
 * The palette renders every project's pages once a query exists, which is
 * several hundred rows on a real instance, and a matcher that accepts any
 * scattered subsequence hands back most of them for a three-letter query.
 *
 * Two rules do the work: a term has to land as one run of letters, on a word
 * boundary or inside the text, never scattered, and *every* term has to land.
 * The second is what makes "acme iss" narrow to one row where "iss" alone
 * cannot.
 *
 * Separators are the one thing a run may cross, so "apitoken" still reaches
 * "API Tokens" and "api-token" reaches it back. Those matches score below
 * every match that respects the boundary, which is what keeps them at the
 * bottom of the list rather than out of it.
 */
function scoreCommand(haystack: string, query: string): number {
  const terms = normalize(query).split(/\s+/).filter(Boolean);
  if (terms.length === 0) return 1;

  const target = normalize(haystack);
  const words = target.split(/[^a-z0-9]+/).filter(Boolean);
  const forms: Forms = {
    target,
    words,
    acronym: words.map((word) => word[0]).join(''),
    squashed: words.join(''),
  };

  let total = 0;
  for (const term of terms) {
    const score = scoreTerm(forms, term, squash(term));
    if (score === 0) return 0;
    total += score;
  }

  return total / terms.length;
}

/**
 * cmdk scores each rendered row through this. `value` carries the project name
 * and page, `keywords` the synonyms that never appear on screen, and both are
 * searchable.
 */
export const filterCommand = (
  value: string,
  search: string,
  keywords?: string[],
) => scoreCommand([value, ...(keywords ?? [])].join(' '), search);
