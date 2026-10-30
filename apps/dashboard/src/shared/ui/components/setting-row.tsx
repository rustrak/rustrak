import { Skeleton } from '@rustrak/ui';
import { cn } from '@/shared/lib/utils';

/**
 * One setting: label and explanation on the left, control on the right.
 *
 * Settings pages are long lists of small controls, so each one being a Card
 * adds a border and a heading per field and turns a 10-field page into 10
 * boxes. Rows separated by a hairline read as one list and keep the emphasis
 * on the controls.
 */
export function SettingRow({
  title,
  description,
  htmlFor,
  wide,
  children,
  className,
}: {
  title: string;
  description?: string;
  /** Points the title at its control, so clicking the title focuses it. */
  htmlFor?: string;
  /** A control that is more than one input wide: a list of headers. */
  wide?: boolean;
  children: React.ReactNode;
  className?: string;
}) {
  // The two-column layout follows the row's own width, not the window's:
  // beside the app and settings sidebars a wide window still leaves a narrow
  // row, and a fixed-width control would crush the text into a tall sliver.
  return (
    <div
      className={cn(
        '@container border-b border-border-divider px-5 py-4 last:border-b-0',
        className,
      )}
    >
      <div className="flex flex-col gap-3 @xl:flex-row @xl:items-start @xl:justify-between @xl:gap-8">
        <div className="min-w-0 @xl:max-w-xs">
          {htmlFor ? (
            <label
              htmlFor={htmlFor}
              className="cursor-pointer text-value font-medium text-fg"
            >
              {title}
            </label>
          ) : (
            <p className="text-value font-medium text-fg">{title}</p>
          )}
          {description && (
            <p className="mt-1 text-hint text-fg-subtle">{description}</p>
          )}
        </div>
        <div
          className={cn(
            'shrink-0',
            wide ? '@xl:w-[26rem] @xl:shrink' : '@xl:w-72',
          )}
        >
          {children}
        </div>
      </div>
    </div>
  );
}

/**
 * Groups related rows under a heading, e.g. "Danger Zone": the heading above,
 * the rows inside one card, so a page reads as a few groups rather than a
 * long undivided list.
 *
 * Spacing is owned here, not by each page: a section right under the page
 * header sits at the page's own gap, and sections after it keep a constant
 * distance from each other.
 */
export function SettingSection({
  title,
  description,
  destructive,
  actions,
  children,
}: {
  title: string;
  description?: React.ReactNode;
  destructive?: boolean;
  /** Beside the heading: the section's own action, "Invite", "New key". */
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="mt-4 first:mt-0 [.settings-header+&]:mt-0">
      <div className="flex items-end justify-between gap-4">
        <div className="min-w-0">
          <h2
            className={cn(
              'text-card-title',
              destructive ? 'text-danger-fg' : 'text-fg',
            )}
          >
            {title}
          </h2>
          {description && (
            <p className="mt-1 max-w-2xl text-meta text-fg-subtle">
              {description}
            </p>
          )}
        </div>
        {actions ? (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        ) : null}
      </div>
      <div
        className={cn(
          'mt-3 overflow-hidden rounded-2xl border bg-surface',
          destructive ? 'border-danger-surface' : 'border-border-subtle',
        )}
      >
        {children}
      </div>
    </section>
  );
}

/**
 * One thing in a list inside a `SettingSection`: a member, a token, a key, a
 * connected app. Leading mark, a name with one line under it, quiet facts on
 * the right, and the row's own actions at the end.
 */
export function SettingsListItem({
  leading,
  title,
  description,
  meta,
  actions,
  className,
}: {
  leading?: React.ReactNode;
  title: React.ReactNode;
  description?: React.ReactNode;
  /** Right-aligned facts: a role, a date, a count. */
  meta?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        'flex min-h-row items-center gap-3 border-b border-border-divider px-5 py-3 last:border-b-0',
        className,
      )}
    >
      {leading ? <div className="shrink-0">{leading}</div> : null}
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="min-w-0 truncate text-value font-medium text-fg">
          {title}
        </div>
        {description ? (
          <div className="min-w-0 truncate text-hint text-fg-subtle">
            {description}
          </div>
        ) : null}
      </div>
      {meta ? (
        <div className="hidden shrink-0 items-center gap-3 text-meta text-fg-meta sm:flex">
          {meta}
        </div>
      ) : null}
      {actions ? (
        <div className="flex shrink-0 items-center gap-1">{actions}</div>
      ) : null}
    </div>
  );
}

/**
 * A `SettingsListItem` list while it loads: the same rows, bars where the
 * name, its line and the facts will be, so the section does not resize when
 * the list lands.
 */
export function SettingsListSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <>
      {Array.from({ length: rows }, (_, row) => (
        <SettingsListItem
          // Placeholder rows have no identity beyond their position.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          key={row}
          leading={<Skeleton className="size-8 rounded-lg" />}
          title={<Skeleton className="my-0.5 h-4 w-48" />}
          description={<Skeleton className="my-0.5 h-3 w-32" />}
          meta={<Skeleton className="h-3 w-24" />}
        />
      ))}
    </>
  );
}

/**
 * What a section shows when it has nothing in it yet: what would be here,
 * and the one action that puts something there.
 */
export function SettingsEmpty({
  icon,
  title,
  description,
  action,
}: {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-1.5 px-6 py-12 text-center">
      {icon ? <div className="mb-2 text-fg-ghost">{icon}</div> : null}
      <p className="text-value font-medium text-fg">{title}</p>
      {description ? (
        <p className="max-w-sm text-meta text-fg-subtle">{description}</p>
      ) : null}
      {action ? <div className="mt-3">{action}</div> : null}
    </div>
  );
}
