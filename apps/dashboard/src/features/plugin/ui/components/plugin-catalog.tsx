import type {
  PluginSummary,
  RegistryListing,
  RegistryPlugin,
} from '@rustrak/client';
import {
  Button,
  confirm,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  QueryBar,
  type QueryField,
  SegmentedControl,
  SegmentedItem,
  SettingsEmpty,
  Skeleton,
  Tag,
} from '@rustrak/ui';
import {
  type UseQueryResult,
  useMutation,
  useQuery,
} from '@tanstack/react-query';
import type { ColumnFiltersState } from '@tanstack/react-table';
import { type ChangeEvent, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useLocale, useTranslations } from 'use-intl';
import type { ApiError } from '@/shared/api/query';
import { cn } from '@/shared/lib/utils';
import { Link } from '@/shared/ui/components/link';
import { QueryState } from '@/shared/ui/components/query-state';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { pluginMutations } from '../../api/mutations';
import { pluginQueries } from '../../api/queries';
import {
  type CatalogEntry,
  type CatalogFilter,
  catalog,
  hasUpdate,
  pluginSettingsHref,
  readmeIn,
} from '../../model/plugins';
import { PluginMark } from './plugin-icon';
import { PluginReadme } from './plugin-readme';

/**
 * Installing from a `.rtp` file: someone's own plugin, not verified. The
 * reader is told so, and agrees, before the file picker opens.
 */
export function InstallPluginButton() {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const install = useMutation(pluginMutations.install());
  const file = useRef<HTMLInputElement>(null);

  const pick = async () => {
    const ok = await confirm({
      title: t('plugins.local.title'),
      description: t('plugins.local.description'),
      tone: 'danger',
      confirmLabel: t('plugins.local.confirm'),
      cancelLabel: t('plugins.detail.cancel'),
    });
    if (ok) file.current?.click();
  };

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
        variant="secondary"
        disabled={install.isPending}
        onClick={() => void pick()}
      >
        {install.isPending ? t('plugins.installing') : t('plugins.install')}
      </Button>
    </>
  );
}

const NO_FIELDS: QueryField[] = [];
const NO_FILTERS: ColumnFiltersState = [];
const FILTERS: CatalogFilter[] = ['all', 'installed', 'available', 'updates'];

/**
 * Settings › Plugins: the installed plugins and, for an owner, what the
 * signed registry publishes, in one searchable grid. An installed one opens
 * its own page; a published one opens its README, to read before installing.
 */
export function PluginCatalog({
  plugins,
  canInstall,
}: {
  plugins: PluginSummary[];
  canInstall: boolean;
}) {
  const t = useTranslations('settings');
  const registry = useQuery({
    ...pluginQueries.registry(),
    enabled: canInstall,
  });
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<CatalogFilter>('all');
  const [open, setOpen] = useState<CatalogEntry | null>(null);

  const published = registry.data?.available
    ? registry.data.plugins
    : undefined;
  const count = (f: CatalogFilter) => catalog(plugins, published, f, '').length;
  const entries = catalog(plugins, published, filter, query);

  if (count('all') === 0 && !registry.isLoading) {
    return (
      <>
        <RegistryNotice registry={registry} />
        <SettingsEmpty
          title={t('plugins.empty')}
          description={t('plugins.emptyHint')}
        />
      </>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <QueryBar
          className="min-w-0 max-sm:basis-full"
          fields={NO_FIELDS}
          filters={NO_FILTERS}
          search={query}
          placeholder={t('plugins.search')}
          onChange={(next) => setQuery(next.search)}
        />
        {canInstall ? (
          <SegmentedControl
            aria-label={t('plugins.filter.label')}
            value={filter}
            onValueChange={(v) => setFilter(v as CatalogFilter)}
          >
            {FILTERS.map((f) => (
              <SegmentedItem key={f} value={f}>
                {t(`plugins.filter.${f}`)}
                <span className="ms-1.5 text-fg-subtle tabular-nums">
                  {count(f)}
                </span>
              </SegmentedItem>
            ))}
          </SegmentedControl>
        ) : null}
      </div>
      <RegistryNotice registry={registry} />
      {entries.length === 0 && !registry.isLoading ? (
        <SettingsEmpty title={t('plugins.noMatch')} />
      ) : (
        <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2 2xl:grid-cols-3">
          {entries.map((entry) => (
            <PluginCard
              key={entry.id}
              entry={entry}
              onOpen={() => setOpen(entry)}
            />
          ))}
          {registry.isLoading && canInstall
            ? [0, 1].map((i) => (
                <Skeleton key={i} className="h-[118px] rounded-2xl" />
              ))
            : null}
        </div>
      )}
      <PluginPreview entry={open} onClose={() => setOpen(null)} />
    </div>
  );
}

/** Why the published plugins are missing, when the registry did not answer. */
function RegistryNotice({
  registry,
}: {
  registry: UseQueryResult<RegistryListing, ApiError>;
}) {
  const t = useTranslations('settings');
  const unavailable = registry.isError || registry.data?.available === false;
  if (!unavailable) return null;
  return (
    <p className="text-meta text-warning-fg" role="status">
      {registry.isError
        ? t('plugins.registryLoadFailed')
        : t('plugins.registryUnavailable')}
      {registry.data?.error ? `: ${registry.data.error}` : null}
    </p>
  );
}

const CARD = cn(
  'flex flex-col gap-3 rounded-2xl border border-border-subtle bg-surface p-5 text-left outline-none',
  'transition-[border-color] duration-instant ease-standard',
  'hover:border-border-strong focus-visible:ring-2 focus-visible:ring-ring',
);

function PluginCard({
  entry,
  onOpen,
}: {
  entry: CatalogEntry;
  onOpen: () => void;
}) {
  const body = <CardBody entry={entry} />;
  // An installed plugin with nothing new opens its own page; anything the
  // registry has to offer opens its README first.
  return entry.installed && !hasUpdate(entry) ? (
    <Link href={pluginSettingsHref(entry.id)} className={CARD}>
      {body}
    </Link>
  ) : (
    <button
      type="button"
      onClick={onOpen}
      className={CARD}
      data-testid="plugin-card"
    >
      {body}
    </button>
  );
}

function CardBody({ entry }: { entry: CatalogEntry }) {
  const shown = entry.installed ?? entry.published;
  if (!shown) return null;
  return (
    <>
      <div className="flex items-start gap-3">
        <PluginMark logo={shown.logo} icon={shown.icon} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-section text-fg">
            {entry.name}
          </span>
          <span className="block truncate text-meta text-fg-subtle">
            <VersionLine plugin={shown} />
          </span>
        </span>
        <Status entry={entry} />
      </div>
      {entry.description ? (
        <p className="line-clamp-2 text-meta text-fg-subtle">
          {entry.description}
        </p>
      ) : null}
      <Verification verified={entry.installed?.verified ?? true} />
    </>
  );
}

function Status({ entry }: { entry: CatalogEntry }) {
  const t = useTranslations('settings');
  if (!entry.installed) {
    return (
      <Tag variant="soft" tone="info">
        {t('plugins.notInstalled')}
      </Tag>
    );
  }
  if (hasUpdate(entry)) {
    return (
      <Tag variant="soft" tone="warning">
        {t('plugins.updateAvailable')}
      </Tag>
    );
  }
  const active =
    entry.installed.allProjects || entry.installed.projects.length > 0;
  return (
    <Tag variant="soft" tone={active ? 'brand' : 'neutral'}>
      {active ? t('plugins.active') : t('plugins.inactive')}
    </Tag>
  );
}

/**
 * A published plugin's README, read from the package the signed index lists,
 * with the button that installs (or updates to) that package.
 */
function PluginPreview({
  entry,
  onClose,
}: {
  entry: CatalogEntry | null;
  onClose: () => void;
}) {
  const t = useTranslations('settings');
  const locale = useLocale();
  const toastFailure = useToastFailure();
  const install = useMutation(pluginMutations.installFromRegistry());
  const published = entry?.published;
  const readme = useQuery({
    ...pluginQueries.registryReadme(published?.id ?? ''),
    enabled: !!published,
  });

  const onInstall = () => {
    if (!published) return;
    install.mutate(published.id, {
      onSuccess: (installed) => {
        toast.success(
          t('plugins.installed', {
            name: installed.name,
            version: installed.version,
          }),
        );
        onClose();
      },
      onError: (error) => toastFailure(t('plugins.installFailed'), error),
    });
  };

  return (
    <Dialog
      open={!!published}
      onOpenChange={(next) => (next ? null : onClose())}
      size="lg"
    >
      {published ? (
        <>
          <DialogHeader
            title={published.name}
            description={<PublishedVersion plugin={published} />}
          />
          <DialogBody className="max-h-[60vh] overflow-y-auto p-0">
            <QueryState
              query={readme}
              placeholder={
                <div className="flex flex-col gap-3 px-6 py-5">
                  <Skeleton className="h-6 w-1/3" />
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-2/3" />
                </div>
              }
              title={t('plugins.readmeFailed')}
            >
              {(md) => (
                <PluginReadme markdown={readmeIn(md, locale)} untitled />
              )}
            </QueryState>
          </DialogBody>
          <DialogFooter hint={<Verification verified />}>
            {entry?.installed ? (
              <Button
                variant="secondary"
                render={<Link href={pluginSettingsHref(published.id)} />}
              >
                {t('plugins.openSettings')}
              </Button>
            ) : null}
            <Button
              variant="primary"
              disabled={install.isPending}
              onClick={onInstall}
            >
              {install.isPending
                ? t('plugins.installing')
                : published.installed_version
                  ? t('plugins.updateButton')
                  : t('plugins.installButton')}
            </Button>
          </DialogFooter>
        </>
      ) : null}
    </Dialog>
  );
}

function PublishedVersion({ plugin }: { plugin: RegistryPlugin }) {
  const t = useTranslations('settings');
  return plugin.installed_version ? (
    t('plugins.update', { from: plugin.installed_version, to: plugin.version })
  ) : (
    <VersionLine plugin={plugin} />
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
