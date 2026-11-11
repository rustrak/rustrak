/**
 * Shared predicates for the architecture suite: pure string functions that
 * archunit's `adhereTo` calls on the `FileInfo` it hands over.
 */

/** A path with forward slashes, whatever the OS wrote. */
export const posix = (path: string) => path.split('\\').join('/');

/**
 * Whether a path is test code, which every rule here exempts: tests
 * legitimately do what the rules forbid.
 *
 * **Matched on `path`, never on `name`.** archunit's `FileInfo` splits
 * `auth.test.ts` into `name: 'auth.test'` and `extension: 'ts'`, so
 * `name.endsWith('.test.ts')` is false for every file.
 */
export function isTestFile(path: string): boolean {
  const normalised = posix(path);
  return (
    normalised.includes('/__tests__/') ||
    /\.(test|spec)\.tsx?$/.test(normalised)
  );
}

/** Whether a path is one of the message catalogues. */
export const isMessages = (path: string) => posix(path).includes('/messages/');

/**
 * The file with its comments blanked out, so a rule does not fire on the
 * prose documenting it. Comments become equivalent whitespace, so offsets
 * still line up.
 *
 * Known limit: a `//` inside a string literal, as in a URL, blanks the rest of
 * that line. That can only hide a violation sharing a line with a URL.
 */
export function withoutComments(source: string): string {
  const blank = (match: string) => match.replace(/[^\n]/g, ' ');

  return source
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/\/\/[^\n]*/g, blank);
}
