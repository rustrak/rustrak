import type { Project } from '@rustrak/client';
import { Link } from '@tanstack/react-router';
import { MoreVertical, Trash2 } from 'lucide-react';
import { PlatformIcon } from 'platformicons';
import { useFormatter, useTranslations } from 'use-intl';
import { PROJECT_COLUMNS } from '@/features/project/model/columns';
import { ProjectStatsCells } from '@/features/project/ui/components/project-stats-cells';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Checkbox } from '@/shared/ui/components/shadcn/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/shared/ui/components/shadcn/dropdown-menu';

export function ProjectRow({
  project,
  selected,
  onToggleSelect,
  onDelete,
}: {
  project: Project;
  selected: boolean;
  onToggleSelect: () => void;
  onDelete: (project: Project) => void;
}) {
  const format = useFormatter();
  const t = useTranslations('projects');

  return (
    <div className="flex items-center gap-4 px-4 py-4 border-b last:border-b-0 hover:bg-muted/30 transition-colors group">
      <Checkbox checked={selected} onCheckedChange={onToggleSelect} />

      <PlatformIcon
        platform={project.platform ?? 'other'}
        size={28}
        radius={5}
        format="lg"
        className="shrink-0"
      />

      <div className={PROJECT_COLUMNS.name}>
        <Link
          to="/projects/$id"
          params={{ id: project.id }}
          className="block group-hover:text-primary transition-colors"
        >
          {/* The DSN used to sit here. It is a secret-ish connection string
              nobody reads at a glance, it forced the name field to hog the
              row, and it already has a home in settings/client-keys. */}
          <div className="font-semibold text-base truncate">{project.name}</div>
          <div className="font-mono text-xs text-muted-foreground truncate">
            {project.slug}
          </div>
        </Link>
      </div>

      <ProjectStatsCells
        stats={project.stats}
        totalEvents={project.digested_event_count}
      />

      <div className={PROJECT_COLUMNS.created}>
        <span className="text-sm text-muted-foreground">
          {format.relativeTime(new Date(project.created_at))}
        </span>
      </div>

      <DropdownMenu>
        <DropdownMenuTrigger
          render={<Button variant="ghost" size="icon" className="size-8" />}
        >
          <MoreVertical className="size-4" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem
            variant="destructive"
            onClick={() => onDelete(project)}
          >
            <Trash2 className="mr-2 size-4" />
            {t('actions.delete')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
