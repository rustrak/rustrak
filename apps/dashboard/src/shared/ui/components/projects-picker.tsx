import { Button, Checkbox, ChevronDownIcon, Popover } from '@rustrak/ui';
import { PlatformIcon } from 'platformicons';
import { useTranslations } from 'use-intl';

export type ProjectOption = {
  id: number;
  name: string;
  platform?: string | null;
};

/** A project's platform logo, the one the projects list shows. */
export function ProjectLogo({
  project,
  size = 18,
}: {
  project: Pick<ProjectOption, 'platform'>;
  size?: 16 | 18;
}) {
  return (
    <PlatformIcon
      platform={project.platform ?? 'other'}
      size={size}
      format="lg"
      className="shrink-0 rounded"
    />
  );
}

/**
 * Where something applies: every project, including ones created later
 * (`null`), or only the ones ticked.
 */
export function ProjectsPicker({
  projects,
  value,
  onChange,
  readOnly,
  allLabel,
  allHint,
  compact = false,
}: {
  projects: ProjectOption[];
  value: number[] | null;
  onChange: (next: number[] | null) => void;
  readOnly: boolean;
  /** A page's filter rather than a form's field: its own width, smaller. */
  compact?: boolean;
  /** What "every project" means here, when not all there are. */
  allLabel?: string;
  allHint?: string;
}) {
  const t = useTranslations('access');
  const all = allLabel ?? t('roles.allProjects');
  const chosen = new Set(value);
  const picked = projects.filter((p) => chosen.has(p.id));
  // Projects the reader cannot see still count, they just have no name here.
  const label =
    value === null
      ? all
      : value.length === 0
        ? t('roles.noProjects')
        : picked.length === value.length && picked.length <= 2
          ? picked.map((p) => p.name).join(', ')
          : t('roles.projectCount', { count: value.length });

  const toggle = (id: number, on: boolean) => {
    const current = value ?? projects.map((p) => p.id);
    onChange(on ? [...current, id] : current.filter((v) => v !== id));
  };

  return (
    <Popover
      align="end"
      popupClassName="w-72 p-1"
      trigger={
        <Button
          variant="secondary"
          disabled={readOnly}
          size={compact ? 'md' : 'lg'}
          className={compact ? 'max-w-64' : 'w-full'}
        >
          <span className="flex w-full min-w-0 items-center gap-2">
            {value !== null && picked.length > 0 ? (
              <span className="flex -space-x-1">
                {picked.slice(0, 3).map((p) => (
                  <ProjectLogo key={p.id} project={p} />
                ))}
              </span>
            ) : null}
            <span className="flex-1 truncate text-start">{label}</span>
            <ChevronDownIcon size="sm" className="text-fg-subtle" />
          </span>
        </Button>
      }
    >
      <label className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2 hover:bg-surface-hover">
        <Checkbox
          aria-label={all}
          checked={value === null}
          onCheckedChange={(on) => onChange(on ? null : [])}
        />
        <span className="flex flex-col">
          <span className="text-control text-fg">{all}</span>
          <span className="text-fg-subtle text-meta">
            {allHint ?? t('roles.allProjectsHint')}
          </span>
        </span>
      </label>
      <div className="my-1 h-px bg-border-subtle" />
      <div className="max-h-64 overflow-y-auto">
        {projects.length === 0 ? (
          <p className="px-2.5 py-2 text-fg-subtle text-meta">
            {t('roles.noProjectsYet')}
          </p>
        ) : (
          projects.map((p) => (
            <label
              key={p.id}
              className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-1.5 hover:bg-surface-hover"
            >
              <Checkbox
                aria-label={p.name}
                checked={value === null || chosen.has(p.id)}
                onCheckedChange={(on) => toggle(p.id, on)}
              />
              <ProjectLogo project={p} />
              <span className="truncate text-control text-fg">{p.name}</span>
            </label>
          ))
        )}
      </div>
    </Popover>
  );
}
