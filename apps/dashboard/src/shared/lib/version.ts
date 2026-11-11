export interface UpdateInfo {
  current: string;
  latest: string;
  description: string;
  url: string;
}

/**
 * The outcome of one update check.
 *
 * Four outcomes rather than `UpdateInfo | null`, because that `null` was
 * carrying three unrelated meanings at once: the operator switched the check
 * off, the check ran and you are on the newest release, and the check could not
 * run at all. A caller could not tell them apart, so it could not treat them
 * differently, and the only reason that was survivable is that the single
 * caller rendered nothing for all three.
 *
 * `unknown` is the one that has to exist. Its `reason` says which half of the
 * check failed: `server-version` means the running version could not be read
 * (there is nothing to compare against), `feed` means the published release list
 * could not be read (there is nothing to compare with). Neither is "you are up
 * to date", and neither may be rendered as one.
 */
export type UpdateCheck =
  | { state: 'update-available'; info: UpdateInfo }
  | { state: 'up-to-date' }
  | { state: 'disabled' }
  | { state: 'unknown'; reason: 'server-version' | 'feed' };

function normalizeVersion(version: string): string {
  return version.trim().replace(/^v/i, '');
}

function parse(version: string): [number, number, number] | null {
  const match = /^(\d+)\.(\d+)\.(\d+)/.exec(normalizeVersion(version));
  if (!match) return null;
  const [, major, minor, patch] = match;
  return [Number(major), Number(minor), Number(patch)];
}

/**
 * Compares two semver-ish strings, tolerating a leading `v`.
 *
 * Returns 0 when either side cannot be parsed, so an unrecognized version can
 * never be mistaken for an available update.
 */
export function compareVersions(a: string, b: string): number {
  const left = parse(a);
  const right = parse(b);
  if (!left || !right) return 0;

  const [leftMajor, leftMinor, leftPatch] = left;
  const [rightMajor, rightMinor, rightPatch] = right;

  if (leftMajor !== rightMajor) return leftMajor < rightMajor ? -1 : 1;
  if (leftMinor !== rightMinor) return leftMinor < rightMinor ? -1 : 1;
  if (leftPatch !== rightPatch) return leftPatch < rightPatch ? -1 : 1;
  return 0;
}
