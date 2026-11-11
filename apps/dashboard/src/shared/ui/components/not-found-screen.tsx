import { Button } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { ErrorScreen } from '@/shared/ui/components/error-screen';
import { Link } from '@/shared/ui/components/link';

/**
 * The one 404 for every route: an unmatched URL and every missing record the
 * application finds for itself (`LoadFailure` renders it in place). It
 * replaces the header for a signed-in reader too, so the action below is the
 * only way back.
 */
export function NotFoundScreen() {
  const t = useTranslations('errors');
  return (
    <>
      <title>{t('notFound.meta.title')}</title>
      <ErrorScreen
        brandStatement={t('notFound.brandStatement')}
        brandDescription={t('notFound.brandDescription')}
        headline={t('notFound.headline')}
        description={t('notFound.description')}
        guidance={t('notFound.guidance')}
        actions={
          <Button variant="primary" render={<Link href="/projects" />}>
            {t('goToProjects')}
          </Button>
        }
      />
    </>
  );
}
