import type { ProjectResponse } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';

interface ProjectHeaderProps {
  project: ProjectResponse;
}

export function ProjectHeader({ project }: ProjectHeaderProps) {
  const format = useFormatter();
  const t = useTranslations('projects');

  return (
    <div className="flex items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-xl md:text-2xl font-extrabold tracking-tight truncate">
          {project.name}
        </h1>
        <p className="text-fg-subtle mt-1 font-mono text-sm truncate">
          {project.slug}
        </p>
      </div>

      <div className="text-right shrink-0">
        <p className="text-column text-fg-subtle">{t('events')}</p>
        <p className="text-xl font-bold text-fg-brand">
          {format.number(project.digested_event_count)}
        </p>
      </div>
    </div>
  );
}
