import { useQuery } from '@tanstack/react-query';
import {
  Database,
  FileCode2,
  Layers,
  ListTree,
  ScrollText,
} from 'lucide-react';
import { useFormatter, useTranslations } from 'use-intl';
import { storageQueries } from '@/features/storage/api/queries';
import { SummaryCardsSkeleton } from '@/features/storage/ui/components/storage-skeletons';
import { formatBytes } from '@/shared/lib/utils';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { Card, CardContent } from '@/shared/ui/components/shadcn/card';

/**
 * Overview cards. Owns its own (heavy) summary query so it can stream in behind
 * a skeleton without blocking the rest of the page.
 */
export function StorageSummaryCards() {
  const format = useFormatter();
  const t = useTranslations('settings');
  const { data: result } = useQuery(storageQueries.summary());

  if (!result) return <SummaryCardsSkeleton />;

  if (!result.success) {
    return (
      <LoadFailure
        error={result.error}
        title={t('storage.loadSummaryFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const summary = result.data;

  const cards = [
    {
      id: 'dbSize',
      label: t('storage.dbSize'),
      value: formatBytes(summary.total_db_size_bytes),
      sub: t('storage.eventsCount', {
        count: format.number(summary.events_count),
      }),
      icon: Database,
    },
    {
      id: 'transactions',
      label: t('storage.transactions'),
      value: format.number(summary.transactions_count),
      sub: t('storage.spansCount', {
        count: format.number(summary.spans_count),
      }),
      icon: ListTree,
    },
    {
      id: 'spans',
      label: t('storage.spans'),
      value: format.number(summary.spans_count),
      sub: t('storage.spansSub'),
      icon: Layers,
    },
    {
      id: 'logs',
      label: t('storage.logs'),
      value: format.number(summary.logs_count),
      sub: t('storage.logsSub'),
      icon: ScrollText,
    },
    {
      id: 'sourceMaps',
      label: t('storage.sourceMaps'),
      value: formatBytes(summary.source_maps.total_bytes),
      sub: t('storage.sourceMapsFiles', {
        count: format.number(summary.source_maps.file_count),
      }),
      icon: FileCode2,
    },
  ];

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      {cards.map((card) => {
        const Icon = card.icon;
        return (
          <Card key={card.id} size="sm">
            <CardContent>
              <div className="flex items-center gap-1.5 text-muted-foreground mb-1">
                <Icon className="size-3.5" />
                <span className="text-[11px] font-semibold uppercase tracking-wide">
                  {card.label}
                </span>
              </div>
              <p className="text-xl font-extrabold tracking-tight leading-tight">
                {card.value}
              </p>
              <p className="text-muted-foreground mt-0.5 text-xs">{card.sub}</p>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
