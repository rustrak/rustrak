import type { RetentionSettings } from '@rustrak/client';
import { Button, EditIcon, Input, SearchIcon, Tag } from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  KINDS,
  type ProjectShare,
  WINDOW_FIELD,
  windowsFor,
} from '@/features/storage/model/retention';
import { formatBytes } from '@/shared/lib/utils';
import { SettingSection } from '@/shared/ui/components/setting-row';

/** Header and body cells share their padding; the outer ones meet the card's. */
const CELL = 'px-3 py-3 first:pl-5 last:pr-5';

/** Past this many projects the table gets a search box. */
const SEARCH_FROM = 8;

/**
 * Which project holds what, largest first, and how long each keeps it.
 */
export function StorageProjects({
  projects,
  settings,
  readOnly,
  onEditRetention,
}: {
  projects: readonly ProjectShare[];
  settings: RetentionSettings;
  /** Without `storage:update`: each project's retention shows, not changes. */
  readOnly: boolean;
  onEditRetention: (project: { id: number; name: string }) => void;
}) {
  const t = useTranslations('storage');
  const [query, setQuery] = useState('');
  const shown = projects.filter((p) =>
    p.name.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const keep = (days: number | null) =>
    days === null ? t('keep.forever') : t('keep.days', { count: days });

  return (
    <SettingSection
      title={t('projects.title')}
      actions={
        projects.length > SEARCH_FROM && (
          <Input
            size="sm"
            boxClassName="w-64"
            leading={<SearchIcon size="sm" aria-hidden="true" />}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t('projects.search')}
            aria-label={t('projects.search')}
          />
        )
      }
    >
      <div className="overflow-x-auto">
        <table className="w-full text-value">
          <thead>
            <tr className="border-b border-border-divider text-start text-label text-fg-meta">
              <th className={`${CELL} text-start font-medium`}>
                {t('projects.project')}
              </th>
              <th className={`${CELL} text-start font-medium sm:w-1/4`}>
                {t('projects.size')}
              </th>
              {KINDS.map((k) => (
                <th
                  key={k}
                  className={`${CELL} hidden text-end font-medium lg:table-cell`}
                >
                  {t(`kind.${k}`)}
                </th>
              ))}
              <th className={`${CELL} text-end font-medium`}>
                {t('projects.retention')}
              </th>
            </tr>
          </thead>
          <tbody>
            {shown.length === 0 && (
              <tr>
                <td
                  colSpan={6}
                  className="px-5 py-8 text-center text-meta text-fg-subtle"
                >
                  {projects.length === 0
                    ? t('projects.empty')
                    : t('projects.noMatch')}
                </td>
              </tr>
            )}
            {shown.map((p) => (
              <tr
                key={p.id}
                className="border-b border-border-divider last:border-b-0 hover:bg-surface-hover"
              >
                <td
                  className={`${CELL} max-w-36 truncate font-medium text-fg sm:max-w-56`}
                >
                  {p.name}
                </td>
                <td className={CELL}>
                  <div className="flex items-center gap-3">
                    <div className="hidden h-1.5 flex-1 overflow-hidden rounded-full bg-surface-chip sm:block">
                      <div
                        className="h-full rounded-full bg-chart-1"
                        style={{ width: `${p.share * 100}%` }}
                      />
                    </div>
                    <span className="text-end font-medium tabular-nums text-fg sm:w-20">
                      {formatBytes(p.bytes)}
                    </span>
                  </div>
                </td>
                {KINDS.map((k) => (
                  <td
                    key={k}
                    className={`${CELL} hidden text-end tabular-nums text-fg-subtle lg:table-cell`}
                  >
                    {formatBytes(p.byKind[k])}
                  </td>
                ))}
                <td className={`${CELL} text-end`}>
                  {/* The value is the action: one tap target, no extra
                      column to crowd a phone. */}
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={readOnly}
                    onClick={() => onEditRetention({ id: p.id, name: p.name })}
                    aria-label={t('projects.change', { project: p.name })}
                    title={
                      p.custom
                        ? KINDS.map(
                            (k) =>
                              `${t(`kind.${k}`)} ${keep(windowsFor(settings, p.id)[WINDOW_FIELD[k]])}`,
                          ).join(' · ')
                        : undefined
                    }
                  >
                    {p.custom ? (
                      <Tag variant="soft" tone="brand">
                        {t('projects.own')}
                      </Tag>
                    ) : (
                      t('projects.general')
                    )}
                    <EditIcon size="sm" aria-hidden="true" />
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </SettingSection>
  );
}
