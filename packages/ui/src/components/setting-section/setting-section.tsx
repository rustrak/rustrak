import type { ReactNode } from 'react';
import { tv } from '../../lib/tv';
import { Skeleton } from '../skeleton/skeleton';

/**
 * Groups related rows under a heading, e.g. "Danger Zone": the heading above,
 * the rows inside one card, so a page reads as a few groups rather than a
 * long undivided list.
 *
 * Spacing is owned here, not by each page: a section right under the page
 * header sits at the page's own gap, and sections after it keep a constant
 * distance from each other.
 */
const section = tv({
  slots: {
    root: 'mt-4 first:mt-0 [.settings-header+&]:mt-0',
    head: 'flex items-end justify-between gap-4',
    title: 'text-card-title',
    description: 'mt-1 max-w-2xl text-meta text-fg-subtle',
    actions: 'flex shrink-0 items-center gap-2',
    body: 'mt-3 overflow-hidden rounded-2xl border bg-surface',
  },
  variants: {
    destructive: {
      true: { title: 'text-danger-fg', body: 'border-danger-surface' },
      false: { title: 'text-fg', body: 'border-border-subtle' },
    },
  },
  defaultVariants: { destructive: false },
});

export interface SettingSectionProps {
  title: string;
  description?: ReactNode;
  destructive?: boolean;
  /** Beside the heading: the section's own action, "Invite", "New key". */
  actions?: ReactNode;
  children: ReactNode;
}

export function SettingSection({
  title,
  description,
  destructive,
  actions,
  children,
}: SettingSectionProps) {
  const s = section({ destructive });
  return (
    <section className={s.root()}>
      <div className={s.head()}>
        <div className="min-w-0">
          <h2 className={s.title()}>{title}</h2>
          {description ? (
            <p className={s.description()}>{description}</p>
          ) : null}
        </div>
        {actions ? <div className={s.actions()}>{actions}</div> : null}
      </div>
      <div className={s.body()}>{children}</div>
    </section>
  );
}

SettingSection.displayName = 'SettingSection';

/**
 * One setting: label and explanation on the left, control on the right.
 *
 * Rows separated by a hairline read as one list and keep the emphasis on the
 * controls. The two-column layout follows the row's own width, not the
 * window's: beside the app and settings sidebars a wide window still leaves a
 * narrow row, and a fixed-width control would crush the text into a sliver.
 */
const row = tv({
  slots: {
    root: '@container border-b border-border-divider px-5 py-4 last:border-b-0',
    layout:
      'flex flex-col gap-3 @xl:flex-row @xl:items-start @xl:justify-between @xl:gap-8',
    text: 'min-w-0 @xl:max-w-xs',
    title: 'text-value font-medium text-fg',
    description: 'mt-1 text-hint text-fg-subtle',
    control: 'shrink-0',
  },
  variants: {
    wide: {
      true: { control: '@xl:w-setting-control-wide @xl:shrink' },
      false: { control: '@xl:w-setting-control' },
    },
    labelled: { true: { title: 'cursor-pointer' } },
  },
  defaultVariants: { wide: false },
});

export interface SettingRowProps {
  title: string;
  description?: string;
  /** Points the title at its control, so clicking the title focuses it. */
  htmlFor?: string;
  /** A control that is more than one input wide: a list of headers. */
  wide?: boolean;
  children: ReactNode;
  className?: string;
}

export function SettingRow({
  title,
  description,
  htmlFor,
  wide,
  children,
  className,
}: SettingRowProps) {
  const s = row({ wide, labelled: Boolean(htmlFor) });
  return (
    <div className={s.root({ className })}>
      <div className={s.layout()}>
        <div className={s.text()}>
          {htmlFor ? (
            <label htmlFor={htmlFor} className={s.title()}>
              {title}
            </label>
          ) : (
            <p className={s.title()}>{title}</p>
          )}
          {description ? (
            <p className={s.description()}>{description}</p>
          ) : null}
        </div>
        <div className={s.control()}>{children}</div>
      </div>
    </div>
  );
}

SettingRow.displayName = 'SettingRow';

/**
 * One thing in a list inside a `SettingSection`: a member, a token, a key, a
 * connected app. Leading mark, a name with one line under it, quiet facts on
 * the right, and the row's own actions at the end.
 */
const item = tv({
  slots: {
    root: 'flex min-h-row items-center gap-3 border-b border-border-divider px-5 py-3 last:border-b-0',
    leading: 'shrink-0',
    text: 'flex min-w-0 flex-1 flex-col gap-0.5',
    title: 'min-w-0 truncate text-value font-medium text-fg',
    description: 'min-w-0 truncate text-hint text-fg-subtle',
    meta: 'hidden shrink-0 items-center gap-3 text-meta text-fg-meta sm:flex',
    actions: 'flex shrink-0 items-center gap-1',
  },
});

export interface SettingsListItemProps {
  leading?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  /** Right-aligned facts: a role, a date, a count. */
  meta?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

export function SettingsListItem({
  leading,
  title,
  description,
  meta,
  actions,
  className,
}: SettingsListItemProps) {
  const s = item();
  return (
    <div className={s.root({ className })}>
      {leading ? <div className={s.leading()}>{leading}</div> : null}
      <div className={s.text()}>
        <div className={s.title()}>{title}</div>
        {description ? (
          <div className={s.description()}>{description}</div>
        ) : null}
      </div>
      {meta ? <div className={s.meta()}>{meta}</div> : null}
      {actions ? <div className={s.actions()}>{actions}</div> : null}
    </div>
  );
}

SettingsListItem.displayName = 'SettingsListItem';

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
const empty = tv({
  slots: {
    root: 'flex flex-col items-center gap-1.5 px-6 py-12 text-center',
    icon: 'mb-2 text-fg-ghost',
    title: 'text-value font-medium text-fg',
    description: 'max-w-sm text-meta text-fg-subtle',
    action: 'mt-3',
  },
});

export interface SettingsEmptyProps {
  icon?: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}

export function SettingsEmpty({
  icon,
  title,
  description,
  action,
}: SettingsEmptyProps) {
  const s = empty();
  return (
    <div className={s.root()}>
      {icon ? <div className={s.icon()}>{icon}</div> : null}
      <p className={s.title()}>{title}</p>
      {description ? <p className={s.description()}>{description}</p> : null}
      {action ? <div className={s.action()}>{action}</div> : null}
    </div>
  );
}
