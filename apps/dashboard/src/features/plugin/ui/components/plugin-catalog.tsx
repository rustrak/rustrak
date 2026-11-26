import type {
  PluginSummary,
  RegistryListing,
  RegistryPlugin,
} from '@rustrak/client';
import {
  Button,
  SettingSection,
  SettingsEmpty,
  SettingsListItem,
  SettingsListSkeleton,
  Tag,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { type ChangeEvent, useRef } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { Link } from '@/shared/ui/components/link';
import { QueryState } from '@/shared/ui/components/query-state';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { pluginMutations } from '../../api/mutations';
import { pluginQueries } from '../../api/queries';
import { pluginSettingsHref } from '../../model/plugins';
import { PluginMark } from './plugin-icon';

/** Installing from a `.rtp` file: someone's own plugin, not verified. */
export function InstallPluginButton() {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const install = useMutation(pluginMutations.install());
  const file = useRef<HTMLInputElement>(null);

  const onFile = (e: ChangeEvent<HTMLInputElement>) => {
    const picked = e.target.files?.[0];
    e.target.value = '';
    if (!picked) return;
    install.mutate(picked, {
      onSuccess: (plugin) =>
        toast.success(
          t('plugins.installed', {
            name: plugin.name,
            version: plugin.version,
          }),
        ),
      onError: (error) => toastFailure(t('plugins.installFailed'), error),
    });
  };

  return (
    <>
      <input
        ref={file}
        type="file"
        accept=".rtp"
        hidden
        onChange={onFile}
        data-testid="plugin-file"
      />
      <Button
        disabled={install.isPending}
        onClick={() => file.current?.click()}
      >
        {install.isPending ? t('plugins.installing') : t('plugins.install')}
      </Button>
    </>
  );
}

/** Settings › Plugins: one card per installed plugin, each to its own page. */
export function PluginCatalog({ plugins }: { plugins: PluginSummary[] }) {
  const t = useTranslations('settings');
  if (plugins.length === 0) {
    return (
      <SettingsEmpty
        title={t('plugins.empty')}
        description={t('plugins.emptyHint')}
      />
    );
  }
  return (
    <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2 2xl:grid-cols-3">
      {plugins.map((plugin) => (
        <PluginCard key={plugin.id} plugin={plugin} />
      ))}
    </div>
  );
}

function PluginCard({ plugin }: { plugin: PluginSummary }) {
  const t = useTranslations('settings');
  const active = plugin.allProjects || plugin.projects.length > 0;
  return (
    <Link
      href={pluginSettingsHref(plugin.id)}
      className={cn(
        'flex flex-col gap-3 rounded-2xl border border-border-subtle bg-surface p-5 outline-none',
        'transition-[border-color] duration-instant ease-standard',
        'hover:border-border-strong focus-visible:ring-2 focus-visible:ring-ring',
      )}
    >
      <div className="flex items-center gap-3">
        <PluginMark logo={plugin.logo} icon={plugin.icon} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-section text-fg">
            {plugin.name}
          </span>
          <span className="block truncate text-meta text-fg-subtle">
            <VersionLine plugin={plugin} />
          </span>
          <Verification verified={plugin.verified} />
        </span>
        <Tag variant="soft" tone={active ? 'brand' : 'neutral'}>
          {active ? t('plugins.active') : t('plugins.inactive')}
        </Tag>
      </div>
      {plugin.description ? (
        <p className="line-clamp-2 text-meta text-fg-subtle">
          {plugin.description}
        </p>
      ) : null}
    </Link>
  );
}

/** `1.2.0 by Acme`, or the version alone. */
export function VersionLine({
  plugin,
}: {
  plugin: { version: string; author?: string | null };
}) {
  const t = useTranslations('settings');
  return plugin.author
    ? t('plugins.versionBy', { version: plugin.version, author: plugin.author })
    : plugin.version;
}

/** Published and signed by the registry, or someone's own upload. */
export function Verification({ verified }: { verified: boolean }) {
  const t = useTranslations('settings');
  return (
    <span
      className={cn(
        'block truncate text-meta',
        verified ? 'text-success-fg' : 'text-warning-fg',
      )}
      data-testid="plugin-verification"
    >
      {verified ? t('plugins.verified') : t('plugins.notVerified')}
    </span>
  );
}

/**
 * What the signed registry publishes and is not installed here at its
 * newest version: installing one is verified.
 */
export function RegistryCatalog({ canInstall }: { canInstall: boolean }) {
  const t = useTranslations('settings');
  const listing = useQuery(pluginQueries.registry());
  const install = useMutation(pluginMutations.installFromRegistry());
  const toastFailure = useToastFailure();
  const offered = (registry: RegistryListing) =>
    registry.plugins.filter((p) => p.installed_version !== p.version);

  return (
    <SettingSection
      title={t('plugins.available')}
      description={t('plugins.availableHint')}
    >
      <QueryState
        query={listing}
        placeholder={<SettingsListSkeleton rows={1} />}
        title={t('plugins.registryLoadFailed')}
      >
        {(registry) =>
          !registry.available ? (
            <SettingsEmpty
              title={t('plugins.registryUnavailable')}
              description={registry.error ?? undefined}
            />
          ) : offered(registry).length === 0 ? (
            <SettingsEmpty title={t('plugins.registryUpToDate')} />
          ) : (
            offered(registry).map((plugin) => (
              <RegistryRow
                key={plugin.id}
                plugin={plugin}
                disabled={!canInstall || install.isPending}
                onInstall={() =>
                  install.mutate(plugin.id, {
                    onSuccess: (installed) =>
                      toast.success(
                        t('plugins.installed', {
                          name: installed.name,
                          version: installed.version,
                        }),
                      ),
                    onError: (error) =>
                      toastFailure(t('plugins.installFailed'), error),
                  })
                }
              />
            ))
          )
        }
      </QueryState>
    </SettingSection>
  );
}

function RegistryRow({
  plugin,
  disabled,
  onInstall,
}: {
  plugin: RegistryPlugin;
  disabled: boolean;
  onInstall: () => void;
}) {
  const t = useTranslations('settings');
  return (
    <SettingsListItem
      leading={<PluginMark logo={plugin.logo} icon={plugin.icon} size="sm" />}
      title={plugin.name}
      description={plugin.description ?? undefined}
      meta={
        plugin.installed_version
          ? t('plugins.update', {
              from: plugin.installed_version,
              to: plugin.version,
            })
          : plugin.version
      }
      actions={
        <Button
          size="sm"
          variant="secondary"
          disabled={disabled}
          onClick={onInstall}
        >
          {plugin.installed_version
            ? t('plugins.updateButton')
            : t('plugins.installButton')}
        </Button>
      }
    />
  );
}
