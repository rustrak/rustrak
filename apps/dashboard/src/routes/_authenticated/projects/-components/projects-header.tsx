import { Link } from '@tanstack/react-router';
import { Plus } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { Button } from '@/shared/ui/components/shadcn/button';

/**
 * Creation lives at `/projects/new`, not in a dialog.
 *
 * Real Sentry uses a full page for this too: `/projects/new/` renders one form
 * with numbered sections (platform, alert frequency, name + team). A dialog
 * has no room for the platform grid, and the flow continues into an SDK setup
 * page afterwards, which a modal cannot lead into.
 *
 * No longer a Client Component: with the form moved out, there is no state
 * left here.
 */
export function ProjectsHeader() {
  const t = useTranslations('projectPages');

  return (
    <div className="flex items-start justify-between gap-4">
      <div>
        <h1 className="text-xl md:text-2xl font-extrabold tracking-tight">
          {t('projectsList.title')}
        </h1>
        <p className="text-muted-foreground mt-1">
          {t('projectsList.subtitle')}
        </p>
      </div>

      <Button nativeButton={false} render={<Link to="/projects/new" />}>
        <Plus className="mr-2 size-4" />
        {t('projectsList.newProject')}
      </Button>
    </div>
  );
}
