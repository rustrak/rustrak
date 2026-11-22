import type { IssueResponse } from '@rustrak/client';
import {
  Button,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Field,
  FieldLabel,
  Select,
  SelectItem,
  Text,
} from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { useIssueChange } from '@/features/issue/ui/components/issue-actions';
import { releaseQueries } from '@/features/release/api/queries';

/**
 * Sentry's "Another existing release": the issue is resolved in the release
 * chosen here, so events from it or a later one reopen it and older ones do
 * not. The page composes it, as it needs both the issue and the releases.
 */
export function ResolveInReleaseDialog({
  open,
  onOpenChange,
  issue,
  projectId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  issue: IssueResponse;
  projectId: number;
}) {
  const t = useTranslations('issues');
  const { update, isPending } = useIssueChange(projectId, issue);
  // Newest first; read only once someone opens the picker.
  const releases = useQuery({
    ...releaseQueries.list(projectId, { per_page: 100 }),
    enabled: open,
  });
  const [version, setVersion] = useState<string | null>(null);
  const versions = releases.data?.items.map((r) => r.version) ?? [];

  const resolve = () => {
    if (!version) return;
    update({ status: 'resolved', statusDetails: { inRelease: version } });
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} size="sm">
      <DialogHeader
        title={t('resolveInRelease.title')}
        description={t('resolveInRelease.description')}
      />
      <DialogBody>
        {releases.isSuccess && versions.length === 0 ? (
          <Text variant="meta" tone="subtle">
            {t('resolveInRelease.empty')}
          </Text>
        ) : (
          <Field>
            <FieldLabel>{t('resolveInRelease.label')}</FieldLabel>
            <Select
              value={version}
              onValueChange={setVersion}
              placeholder={t('resolveInRelease.placeholder')}
              aria-label={t('resolveInRelease.label')}
              disabled={!releases.isSuccess}
            >
              {versions.map((v) => (
                <SelectItem key={v} value={v}>
                  {v}
                </SelectItem>
              ))}
            </Select>
          </Field>
        )}
      </DialogBody>
      <DialogFooter>
        <Button variant="secondary" onClick={() => onOpenChange(false)}>
          {t('resolveInRelease.cancel')}
        </Button>
        <Button
          variant="primary"
          disabled={!version || isPending}
          onClick={resolve}
        >
          {t('resolveInRelease.confirm')}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
