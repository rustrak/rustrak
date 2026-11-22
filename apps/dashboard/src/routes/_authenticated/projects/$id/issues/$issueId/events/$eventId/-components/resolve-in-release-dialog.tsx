import type { IssueResponse } from '@rustrak/client';
import {
  Button,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Field,
  FieldLabel,
  Input,
  Select,
  SelectItem,
  Text,
} from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { useIssueChange } from '@/features/issue/ui/components/issue-actions';
import { releaseQueries } from '@/features/release/api/queries';
import { LoadFailure } from '@/shared/ui/components/load-failure';

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
  // Newest first, narrowed by the server as the reader types, so a project
  // with more releases than one page still reaches all of them. Read only
  // once someone opens the picker.
  const [search, setSearch] = useState('');
  const query = search.trim();
  const releases = useQuery({
    ...releaseQueries.list(projectId, {
      per_page: 100,
      ...(query ? { query } : {}),
    }),
    enabled: open,
  });
  const [version, setVersion] = useState<string | null>(null);
  // Only a release with a record can be resolved in: one known only from
  // its sessions would be refused.
  const versions =
    releases.data?.items.filter((r) => r.registered).map((r) => r.version) ??
    [];

  // A choice the search has since hidden is no choice: nothing the reader
  // cannot see is sent.
  const chosen = version && versions.includes(version) ? version : null;

  const resolve = () => {
    if (!chosen) return;
    update({ status: 'resolved', statusDetails: { inRelease: chosen } });
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} size="sm">
      <DialogHeader
        title={t('resolveInRelease.title')}
        description={t('resolveInRelease.description')}
      />
      <DialogBody className="flex flex-col gap-3">
        <Field>
          <FieldLabel>{t('resolveInRelease.search')}</FieldLabel>
          <Input
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setVersion(null);
            }}
            placeholder={t('resolveInRelease.searchPlaceholder')}
          />
        </Field>
        {releases.isError ? (
          <LoadFailure
            error={releases.error.error}
            title={t('resolveInRelease.loadFailed')}
            notFoundOnMissing={false}
            onRetry={() => releases.refetch()}
          />
        ) : releases.isSuccess && versions.length === 0 ? (
          <Text variant="meta" tone="subtle">
            {query
              ? t('resolveInRelease.noMatch')
              : t('resolveInRelease.empty')}
          </Text>
        ) : (
          <Field>
            <FieldLabel>{t('resolveInRelease.label')}</FieldLabel>
            <Select
              value={chosen}
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
          disabled={!chosen || isPending}
          onClick={resolve}
        >
          {t('resolveInRelease.confirm')}
        </Button>
      </DialogFooter>
    </Dialog>
  );
}
