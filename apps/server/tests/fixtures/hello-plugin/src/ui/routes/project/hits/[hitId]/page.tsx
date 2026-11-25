import {
  type PageProps,
  PluginLink,
  useFormatter,
  usePluginApi,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import {
  EmptyState,
  SettingRow,
  SettingSection,
  SettingsHeader,
} from '@rustrak/ui';
import type { Hit } from '../../../../components/hit-row';

/** One hit, at /projects/:id/plugins/hello/hits/:hitId. */
export default function HitPage({ params }: PageProps<'project'>) {
  const t = usePluginTranslations();
  const format = useFormatter();
  const hits = usePluginApi<Hit[]>('/hits', { limit: 100 });
  const hit = hits.data?.find((h) => String(h.id) === params.hitId);
  if (hits.isSuccess && !hit) {
    return (
      <EmptyState
        title={t('hits.missing')}
        description={<PluginLink to="">{t('hits.back')}</PluginLink>}
      />
    );
  }
  return (
    <div className="flex flex-col gap-4 p-page-gutter" data-testid="hit-page">
      <SettingsHeader
        title={t('hits.detail', { id: params.hitId ?? '' })}
        description={hit?.path}
        actions={
          <PluginLink to="" className="text-meta text-fg-brand">
            {t('hits.back')}
          </PluginLink>
        }
      />
      <SettingSection title={t('hits.title')}>
        <SettingRow title={t('hits.path')}>
          <span className="text-value text-fg">{hit?.path ?? '…'}</span>
        </SettingRow>
        <SettingRow title={t('hits.recordedAt')}>
          <span className="text-value text-fg">
            {hit
              ? format.dateTime(new Date(hit.createdAt * 1000), 'dateTime')
              : '…'}
          </span>
        </SettingRow>
      </SettingSection>
    </div>
  );
}
