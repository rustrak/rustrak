import { Link as RouterLink, useLocation } from '@tanstack/react-router';
import type { ComponentProps } from 'react';
import { projectHrefWithEnvironment } from '@/shared/lib/project-environment';

type RouterLinkProps = ComponentProps<typeof RouterLink>;

/** Accept the app's string URLs, then pass each URL part to TanStack Router. */
export type LinkProps = Omit<
  RouterLinkProps,
  'to' | 'href' | 'search' | 'hash'
> & {
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
  const currentHref = useLocation({ select: (location) => location.href });

  if (isExternal(href)) {
    const { children, ...anchorProps } = props;
    return (
      <a {...(anchorProps as React.ComponentProps<'a'>)} href={href}>
        {children as React.ReactNode}
      </a>
    );
  }

  const target = new URL(
    projectHrefWithEnvironment(href, currentHref),
    'http://localhost',
  );

  return (
    <RouterLink
      {...props}
      to={target.pathname as RouterLinkProps['to']}
      search={
        Object.fromEntries(
          target.searchParams,
        ) as unknown as RouterLinkProps['search']
      }
      hash={target.hash.slice(1)}
      resetScroll={scroll}
    />
  );
}
