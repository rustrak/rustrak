/** How a crash-free rate reads: healthy from 99%, worrying under 95%. */
export function healthTone(
  rate: number | null | undefined,
): 'good' | 'warning' | 'bad' | null {
  if (rate == null) return null;
  if (rate >= 0.99) return 'good';
  if (rate >= 0.95) return 'warning';
  return 'bad';
}

/**
 * A version split the way SDKs name them, `package@version+build`: the
 * package is what the reader already knows, the version what they scan for.
 */
export function splitVersion(raw: string): {
  package: string | null;
  version: string;
} {
  const at = raw.lastIndexOf('@');
  // `@scope/pkg` alone is a package name, not a version.
  if (at <= 0) return { package: null, version: raw };
  return { package: raw.slice(0, at), version: raw.slice(at + 1) };
}
