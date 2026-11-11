import { ChevronRightIcon, cn } from '@rustrak/ui';
import { type ReactNode, useState } from 'react';

/** What this reader last did with a section, by id; `null` if nothing. */
function remembered(id: string | undefined): boolean | null {
  if (!id) return null;
  try {
    const value = window.localStorage.getItem(`rustrak:section:${id}`);
    return value === null ? null : value === 'open';
  } catch {
    return null;
  }
}

/**
 * One section of a long detail page: a heading row that folds it, its own
 * actions on the right, and the body.
 *
 * Folding is remembered per section, in this browser: someone who never
 * reads breadcrumbs folds them once, not on every event. Sections sit flush
 * in one panel, divided by a rule, so a page of eight of them reads as one
 * document rather than eight cards.
 */
export function Section({
  id,
  title,
  defaultOpen = true,
  actions,
  children,
}: {
  id?: string;
  title: string;
  defaultOpen?: boolean;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => remembered(id) ?? defaultOpen);

  return (
    <details
      id={id}
      open={open}
      onToggle={(event) => {
        const next = event.currentTarget.open;
        setOpen(next);
        if (!id) return;
        try {
          window.localStorage.setItem(
            `rustrak:section:${id}`,
            next ? 'open' : 'closed',
          );
        } catch {
          // Storage unavailable: the fold lasts as long as the page.
        }
      }}
      // Clear of the sticky event bar when jumped to.
      className="group/section scroll-mt-32 border-border-divider border-b last:border-b-0"
    >
      <summary
        className={cn(
          'flex h-12 cursor-pointer list-none select-none items-center gap-2 px-5',
          'hover:bg-surface-hover [&::-webkit-details-marker]:hidden',
        )}
      >
        <ChevronRightIcon
          size="sm"
          aria-hidden="true"
          className="shrink-0 text-fg-ghost transition-[rotate] duration-fast ease-standard group-open/section:rotate-90 motion-reduce:transition-none"
        />
        <h2 className="min-w-0 flex-1 truncate text-card-title text-fg">
          {title}
        </h2>
        {/* Clicking an action must not also toggle the section. */}
        {actions ? (
          <div
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            {actions}
          </div>
        ) : null}
      </summary>
      <div className="px-5 pb-5">{children}</div>
    </details>
  );
}
