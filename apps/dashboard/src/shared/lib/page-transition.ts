/**
 * Which view transition a navigation plays on a phone, named by direction.
 *
 * Direction is the meaning, the same rule the sidebar's scope change follows:
 * going deeper (a list to one of its records) the new page arrives from the
 * right, coming back out it arrives from the left, and a move between two
 * pages at the same depth -- the tab bar -- cross-fades, because neither is
 * inside the other. The browser's own Back is "back" whatever the depth, read
 * from the router's history index, so a swipe back never slides the wrong way.
 *
 * A change of query string on the same page (a filter, the next page of a
 * list) is not a navigation to the reader and does not animate.
 */
export type PageTransition = ['slide-forward'] | ['slide-back'] | ['fade'];

interface At {
  pathname: string;
  state: { __TSR_index?: number };
}

const depth = (pathname: string) => pathname.split('/').filter(Boolean).length;

export function pageTransition(
  from: At | undefined,
  to: At,
): PageTransition | false {
  if (!from || from.pathname === to.pathname) return false;

  const fromIndex = from.state.__TSR_index;
  const toIndex = to.state.__TSR_index;
  if (fromIndex !== undefined && toIndex !== undefined && toIndex < fromIndex) {
    return ['slide-back'];
  }

  const delta = depth(to.pathname) - depth(from.pathname);
  if (delta > 0) return ['slide-forward'];
  if (delta < 0) return ['slide-back'];
  return ['fade'];
}
