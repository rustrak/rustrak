import { Link as RouterLink } from '@tanstack/react-router';
import type { ComponentProps } from 'react';

type RouterLinkProps = ComponentProps<typeof RouterLink>;

/**
 * A link, addressed by `href`.
 *
 * TanStack's own `Link` is normally addressed by `to` plus `params`, which is
 * how it type-checks a destination against the route tree. This one takes the
 * built path instead, through the `href` escape hatch the router already
 * provides for exactly that.
 *
 * **That is a deliberate trade, not laziness.** Every navigable component in
 * this app is handed its destination as a string — `item.href` on a sidebar
 * row, `` `/projects/${id}/issues/${issueId}` `` in a table cell — because
 * that is the shape `next/link` took and the shape the design system's
 * `render={<Link .../>}` prop passes through. Rewriting all of them into
 * `to`/`params` pairs is a real improvement and a genuinely separate change;
 * doing it in the same commit as the framework move would have made every
 * visual regression indistinguishable from a routing one.
 *
 * `href` also parses a query string and a hash for free, which `to` does not:
 * `/projects?page=2` is one string here and three properties there.
 */
export type LinkProps = Omit<RouterLinkProps, 'to' | 'href'> & {
  href: string;
  /**
   * Whether following this link scrolls back to the top.
   *
   * `next/link` spelled it `scroll`; the router spells it `resetScroll`. One
   * link in the application sets it — the waterfall row, which selects a span
   * in a pane beside a list the reader has scrolled a long way down, and
   * throwing them back to the top of that list on every selection made the
   * view unusable.
   */
  scroll?: boolean;
};

/**
 * An address the router has no business resolving.
 *
 * The docs link on the tokens page and the repository links on the About page
 * are the ones that matter. `//example.com` counts: it is an absolute URL
 * wearing a path's clothes, and handing it to the router would have it look
 * for a route named after somebody else's host.
 */
function isExternal(href: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//');
}

export function Link({ href, scroll, ...props }: LinkProps) {
  if (isExternal(href)) {
    const { children, ...anchorProps } = props;
    return (
      <a {...(anchorProps as React.ComponentProps<'a'>)} href={href}>
        {children as React.ReactNode}
      </a>
    );
  }

  // A link is a way in: see `useEntered`. A caller's own `state` wins.
  return (
    <RouterLink
      state={{ entry: true }}
      {...props}
      href={href}
      resetScroll={scroll}
    />
  );
}
