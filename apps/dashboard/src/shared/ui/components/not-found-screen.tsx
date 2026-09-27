import { Link } from '@tanstack/react-router';
import { useEffect } from 'react';
import { useTranslations } from 'use-intl';
import { ErrorScreen } from '@/shared/ui/components/error-screen';
import { Button } from '@/shared/ui/components/shadcn/button';

/**
 * The one 404 for every route.
 *
 * It covers both an unmatched URL and every missing record the application
 * finds for itself — a project id that does not exist, a deleted issue, a
 * release with no rows, or `LoadFailure` turning a `not_found` into the app's
 * 404. Because it is the only one, it replaces the header for a signed-in
 * reader too, which is why the action below is not decoration: it is the only
 * way back.
 *
 * It is a component rather than a route so that `LoadFailure` can render it in
 * place. Next raised `notFound()` and the framework swapped the tree; here the
 * two callers — the router's `notFoundComponent` and `LoadFailure` — render
 * the same element, which is the only way they stay identical.
 */
export function NotFoundScreen() {
  const t = useTranslations('errors');
  const title = t('notFound.meta.title');

  // The tab has to say this too, and a route's `head` cannot: a 404 is the
  // absence of a route match, so there is nothing for the router to resolve a
  // title from. Next set it from `not-found.tsx`'s own `generateMetadata` for
  // the same reason, and it replaced the page's title exactly as this does —
  // including when `LoadFailure` renders this screen in place of a record that
  // is gone.
  useEffect(() => {
    const previous = document.title;
    document.title = title;
    return () => {
      document.title = previous;
    };
  }, [title]);

  return (
    <ErrorScreen
      brandStatement={t('notFound.brandStatement')}
      brandDescription={t('notFound.brandDescription')}
      headline={t('notFound.headline')}
      description={t('notFound.description')}
      guidance={t('notFound.guidance')}
      actions={
        <Button nativeButton={false} render={<Link to="/projects" />}>
          {t('goToProjects')}
        </Button>
      }
    />
  );
}
