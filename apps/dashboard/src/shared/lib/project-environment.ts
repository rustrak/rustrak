/** Keep the project environment while following links inside the same project. */
export function projectHrefWithEnvironment(
  href: string,
  current: string,
): string {
  const from = new URL(current, 'http://localhost');
  const to = new URL(href, from);
  const source = from.pathname.match(/^\/projects\/(\d+)(?:\/|$)/);
  const target = to.pathname.match(/^\/projects\/(\d+)(?:\/|$)/);
  const environment = from.searchParams.get('environment');
  if (
    source &&
    target &&
    source[1] === target[1] &&
    environment &&
    !to.searchParams.has('environment')
  ) {
    to.searchParams.set('environment', environment);
  }
  return `${to.pathname}${to.search}${to.hash}`;
}
