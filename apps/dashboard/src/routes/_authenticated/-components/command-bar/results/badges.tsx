import { PlatformIcon } from 'platformicons';
import type { CommandProject } from '../commands';

const BADGE =
  'flex min-w-0 shrink-0 items-center gap-1.5 font-mono text-fg-subtle text-mono-sm';

/**
 * Marks where a row lives once the search flattens everything into one list.
 * Without it "Issues" appears once per project with nothing to tell them
 * apart, which is the state the grouped-by-project version was hiding behind
 * three headings for three rows.
 */
export function ProjectBadge({ project }: { project: CommandProject }) {
  return (
    <span className={BADGE}>
      <PlatformIcon
        platform={project.platform ?? 'other'}
        size={12}
        format="lg"
        className="shrink-0 rounded-[2px]"
      />
      <span className="max-w-32 truncate">{project.name}</span>
    </span>
  );
}

/** The counterpart for rows that belong to the instance, not to a project. */
export function ScopeBadge({ label }: { label: string }) {
  return <span className={BADGE}>{label}</span>;
}
