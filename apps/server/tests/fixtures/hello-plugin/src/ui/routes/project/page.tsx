import {
  definePage,
  usePluginApi,
  usePluginMutation,
  usePluginPermissions,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import {
  Button,
  Figure,
  FigureStrip,
  Input,
  SettingSection,
  SettingsEmpty,
  SettingsHeader,
} from '@rustrak/ui';
import { type FormEvent, useState } from 'react';
import { type Hit, HitRow } from '../../components/hit-row';

export const config = definePage({
  sidebar: { label: 'nav.hits', icon: 'performance', rank: 1 },
});

export default function HitsPage() {
  const t = usePluginTranslations();
  const count = usePluginApi<string>('/count');
  const hits = usePluginApi<Hit[]>('/hits', { limit: 10 });
  const add = usePluginMutation<{ path: string }, Hit>('/hits');
  const clear = usePluginMutation<void>('/hits', 'DELETE');
  const [path, setPath] = useState('');
  const { can } = usePluginPermissions();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    add.mutate({ path }, { onSuccess: () => setPath('') });
  };

  return (
    <div className="flex flex-col gap-4 p-page-gutter" data-testid="hello-page">
      <SettingsHeader
        title={t('hits.title')}
        description={t('hits.subtitle')}
      />
      <FigureStrip className="grid-cols-1">
        <Figure label={t('hits.total')} value={count.data ?? '…'} />
      </FigureStrip>
      <SettingSection title={t('hits.add')}>
        <form onSubmit={submit} className="flex gap-2 p-5">
          <Input
            aria-label={t('hits.path')}
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="/pricing"
            invalid={add.isError}
            className="flex-1"
          />
          <Button type="submit" disabled={add.isPending}>
            {t('hits.addButton')}
          </Button>
        </form>
        {add.error ? (
          <p role="alert" className="px-5 pb-4 text-meta text-danger-fg">
            {add.error.message}
          </p>
        ) : null}
      </SettingSection>
      <SettingSection
        title={t('hits.latest')}
        actions={
          can('hits', 'delete') ? (
            <Button
              variant="secondary"
              onClick={() => clear.mutate()}
              disabled={clear.isPending}
            >
              {t('hits.clear')}
            </Button>
          ) : null
        }
      >
        {hits.data?.length === 0 ? (
          <SettingsEmpty
            title={t('hits.none')}
            description={t('hits.noneHint')}
          />
        ) : (
          <div data-testid="hits">
            {(hits.data ?? []).map((hit) => (
              <HitRow key={hit.id} hit={hit} />
            ))}
          </div>
        )}
      </SettingSection>
    </div>
  );
}
