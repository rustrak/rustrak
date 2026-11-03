import { Button, CopyIcon, InfoIcon, SelectedIcon } from '@rustrak/ui';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { copyToClipboard } from '@/shared/lib/clipboard';

interface EventTagsProps {
  tags: Record<string, string> | undefined;
}

interface TagRowProps {
  tagKey: string;
  tagValue: string;
}

function TagRow({ tagKey, tagValue }: TagRowProps) {
  const t = useTranslations('events');
  const common = useTranslations('common');
  const mcp = useTranslations('settings.mcp');
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (!(await copyToClipboard(`${tagKey}:${tagValue}`))) {
      toast.info(common('clipboardUnavailable'), {
        description: t('clipboardValueHint'),
      });
      return;
    }

    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="group flex items-center gap-4 border-border-divider border-b px-4 py-3 last:border-b-0 hover:bg-surface-hover">
      <div className="w-1/3 min-w-0">
        <span className="block truncate font-medium text-body text-fg-muted">
          {tagKey}
        </span>
      </div>
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className="truncate font-mono text-mono-sm text-fg-secondary">
          {tagValue}
        </span>
        <Button
          variant="ghost"
          size="xs"
          icon={copied ? SelectedIcon : CopyIcon}
          aria-label={mcp('copyLabel', { label: tagKey })}
          onClick={handleCopy}
          className="opacity-0 focus-visible:opacity-100 group-hover:opacity-100"
        />
      </div>
    </div>
  );
}

export function EventTags({ tags }: EventTagsProps) {
  const t = useTranslations('events');

  if (!tags || Object.keys(tags).length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 text-center">
        <InfoIcon size="2xl" className="mb-4 text-fg-ghost" />
        <p className="text-body text-fg-muted">{t('tags.emptyTitle')}</p>
        <p className="mt-1 text-body text-fg-subtle">{t('tags.emptyHint')}</p>
      </div>
    );
  }

  // Group tags into categories
  const categorizedTags: Record<string, Record<string, string>> = {
    deployment: {},
    runtime: {},
    device: {},
    user: {},
    other: {},
  };

  const deploymentKeys = ['environment', 'release', 'server_name', 'site'];
  const runtimeKeys = [
    'runtime',
    'runtime.name',
    'runtime.version',
    'language',
    'sdk.name',
    'sdk.version',
  ];
  const deviceKeys = [
    'browser',
    'browser.name',
    'device',
    'device.family',
    'os',
    'os.name',
    'os.version',
  ];
  const userKeys = [
    'user',
    'user.id',
    'user.email',
    'user.username',
    'user.ip_address',
  ];

  for (const [key, value] of Object.entries(tags)) {
    if (deploymentKeys.some((k) => key.toLowerCase().startsWith(k))) {
      categorizedTags.deployment[key] = value;
    } else if (runtimeKeys.some((k) => key.toLowerCase().startsWith(k))) {
      categorizedTags.runtime[key] = value;
    } else if (deviceKeys.some((k) => key.toLowerCase().startsWith(k))) {
      categorizedTags.device[key] = value;
    } else if (userKeys.some((k) => key.toLowerCase().startsWith(k))) {
      categorizedTags.user[key] = value;
    } else {
      categorizedTags.other[key] = value;
    }
  }

  const sections = [
    {
      key: 'deployment',
      labelKey: 'tags.deployment',
      tags: categorizedTags.deployment,
    },
    { key: 'runtime', labelKey: 'tags.runtime', tags: categorizedTags.runtime },
    {
      key: 'device',
      labelKey: 'tags.device',
      tags: categorizedTags.device,
    },
    { key: 'user', labelKey: 'tags.user', tags: categorizedTags.user },
    { key: 'other', labelKey: 'tags.other', tags: categorizedTags.other },
  ].filter((section) => Object.keys(section.tags).length > 0);

  return (
    <div className="space-y-6">
      {sections.map((section) => (
        <div key={section.key} className="space-y-2">
          <h4 className="text-column text-fg-meta">{t(section.labelKey)}</h4>
          <div className="overflow-hidden rounded-xl border border-border-subtle bg-panel">
            {Object.entries(section.tags).map(([key, value]) => (
              <TagRow key={key} tagKey={key} tagValue={value} />
            ))}
          </div>
        </div>
      ))}

      {/* Summary */}
      <p className="text-meta text-fg-meta">
        {t('tags.attachedCount', { count: Object.keys(tags).length })}
      </p>
    </div>
  );
}
