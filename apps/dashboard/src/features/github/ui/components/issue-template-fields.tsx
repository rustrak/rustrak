import type { IssueTemplate } from '@rustrak/client';
import { Button, Input, Textarea } from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { parseLabels } from '@/features/github/lib/labels';
import { SettingRow } from '@/shared/ui/components/setting-row';

/**
 * The labels, title prefix and body footer a GitHub issue starts with,
 * edited locally and saved together. The parent keys it on the saved value.
 */
export function IssueTemplateFields({
  saved,
  disabled,
  onSave,
}: {
  saved: IssueTemplate;
  disabled: boolean;
  onSave: (template: IssueTemplate) => void;
}) {
  const t = useTranslations('github');
  // A draft of `saved`; a new saved value remounts this component.
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [labels, setLabels] = useState(() => (saved.labels ?? []).join(', '));
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [prefix, setPrefix] = useState(saved.title_prefix ?? '');
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [footer, setFooter] = useState(saved.body_footer ?? '');
  const draft: IssueTemplate = {
    labels: parseLabels(labels),
    title_prefix: prefix,
    body_footer: footer,
  };
  const changed =
    JSON.stringify(draft) !==
    JSON.stringify({
      labels: saved.labels ?? [],
      title_prefix: saved.title_prefix ?? '',
      body_footer: saved.body_footer ?? '',
    });

  return (
    <>
      <SettingRow
        title={t('template.labels')}
        description={t('template.labelsDescription')}
        htmlFor="github-template-labels"
      >
        <Input
          id="github-template-labels"
          value={labels}
          placeholder="bug, rustrak"
          disabled={disabled}
          onChange={(e) => setLabels(e.target.value)}
        />
      </SettingRow>
      <SettingRow
        title={t('template.titlePrefix')}
        description={t('template.titlePrefixDescription')}
        htmlFor="github-template-prefix"
      >
        <Input
          id="github-template-prefix"
          value={prefix}
          placeholder="[Rustrak] "
          disabled={disabled}
          onChange={(e) => setPrefix(e.target.value)}
        />
      </SettingRow>
      <SettingRow
        title={t('template.bodyFooter')}
        description={t('template.bodyFooterDescription')}
        htmlFor="github-template-footer"
      >
        <Textarea
          id="github-template-footer"
          value={footer}
          rows={3}
          placeholder="cc @acme/oncall"
          disabled={disabled}
          onChange={(e) => setFooter(e.target.value)}
        />
      </SettingRow>
      {changed && (
        <div className="flex justify-end px-5 py-3">
          <Button
            variant="primary"
            size="sm"
            disabled={disabled}
            onClick={() => onSave(draft)}
          >
            {t('save')}
          </Button>
        </div>
      )}
    </>
  );
}
