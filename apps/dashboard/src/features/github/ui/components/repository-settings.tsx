import type {
  AlertRuleResponse,
  CodeMapping,
  GitHubRepository,
  ProjectRepository,
} from '@rustrak/client';
import {
  Button,
  CloseIcon,
  ExternalLinkIcon,
  Input,
  NewIcon,
  RefreshIcon,
  Switch,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { githubMutations } from '@/features/github/api/mutations';
import { githubQueries } from '@/features/github/api/queries';
import { savableMappings } from '@/features/github/lib/code-mappings';
import { IssueTemplateFields } from '@/features/github/ui/components/issue-template-fields';
import {
  conditionsFromForm,
  formFromConditions,
} from '@/shared/lib/rule-conditions';
import { ConditionFilters } from '@/shared/ui/components/condition-filters';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { Link } from '@/shared/ui/components/link';
import {
  SettingRow,
  SettingSection,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';

interface RepositorySettingsProps {
  projectId: number;
  projectSlug: string;
  repository: ProjectRepository | null;
}

/**
 * A project's repository, and what follows from it. Only the first row shows
 * until a repository is chosen: everything after it needs one.
 */
export function RepositorySettings({
  projectId,
  projectSlug,
  repository,
}: RepositorySettingsProps) {
  const t = useTranslations('github');
  const format = useFormatter();
  const repositories = useQuery(githubQueries.repositories(projectId));
  const failed = (failure: string) => (error: Error) =>
    toast.error(failure, { description: error.message });
  const setRepository = useMutation({
    ...githubMutations.setRepository(projectId),
    onError: failed(t('repository.saveFailed')),
  });
  const removeRepository = useMutation({
    ...githubMutations.removeRepository(projectId),
    onError: failed(t('repository.saveFailed')),
  });
  const syncCodeowners = useMutation({
    ...githubMutations.syncCodeowners(projectId),
    onError: failed(t('repository.codeownersFailed')),
  });
  // Pending until the saved values are back on screen: a draft keyed on them
  // would otherwise drop edits made between.
  const isPending =
    setRepository.isPending ||
    removeRepository.isPending ||
    syncCodeowners.isPending;

  const save = (changes: {
    repo?: GitHubRepository;
    code_mappings?: CodeMapping[];
    auto_create_issues?: boolean;
    auto_create_conditions?: AlertRuleResponse['conditions'];
    issue_template?: ProjectRepository['issue_template'];
  }) =>
    setRepository.mutate({
      installation_id:
        changes.repo?.installation_id ?? repository?.installation_id ?? 0,
      repo_full_name:
        changes.repo?.full_name ?? repository?.repo_full_name ?? '',
      code_mappings: changes.code_mappings ?? repository?.code_mappings,
      auto_create_issues:
        changes.auto_create_issues ?? repository?.auto_create_issues,
      auto_create_conditions:
        changes.auto_create_conditions ?? repository?.auto_create_conditions,
      issue_template: changes.issue_template ?? repository?.issue_template,
    });

  // Without the list the page still works for a repository already chosen:
  // GitHub being unreachable for a moment must not hide its settings.
  const choices = repositories.data ?? [];
  const unavailable = repositories.isError && (
    <div className="mb-4 flex flex-col gap-1.5 rounded-2xl border border-dashed border-border-subtle bg-surface px-5 py-4">
      <p className="text-value font-medium text-fg">
        {t('repository.unavailable')}
      </p>
      <p className="text-meta text-fg-subtle">{repositories.error.message}</p>
      <Link
        href="/settings/integrations"
        className="inline-flex items-center gap-1 text-meta text-fg-brand hover:underline"
      >
        {t('repository.goToIntegrations')}
        <ExternalLinkIcon size="sm" />
      </Link>
    </div>
  );
  if (unavailable && !repository) return unavailable;

  // Changes the linked repository, or picks the first one.
  const picker = (
    <Select
      value={repository?.repo_full_name ?? ''}
      onValueChange={(value) => {
        const repo = choices.find((r) => r.full_name === value);
        if (repo) save({ repo });
      }}
      disabled={isPending || choices.length === 0}
    >
      <SelectTrigger
        id="github-repository"
        aria-label={t('repository.repository')}
        className="w-full"
      >
        <SelectValue
          placeholder={
            choices.length === 0
              ? t('repository.noRepositories')
              : t('repository.choose')
          }
        />
      </SelectTrigger>
      <SelectContent>
        {choices.map((repo) => (
          <SelectItem key={repo.full_name} value={repo.full_name}>
            {repo.full_name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );

  return (
    <>
      {unavailable}
      <SettingSection title={t('repository.section')}>
        {/* The linked repository as a row of its own, the picker beside it
            to change it; before one is chosen, the picker is the row. */}
        {repository ? (
          <SettingsListItem
            leading={
              <span className="flex size-8 items-center justify-center rounded-lg border border-border-subtle text-fg-secondary">
                <GitHubMark className="size-4" />
              </span>
            }
            title={
              <span className="font-mono text-mono">
                {repository.repo_full_name}
              </span>
            }
            description={
              <a
                href={repository.html_url}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 hover:text-fg"
              >
                {t('repository.openOnGitHub', {
                  branch: repository.default_branch,
                })}
                <ExternalLinkIcon size="sm" />
              </a>
            }
            actions={<div className="w-56">{picker}</div>}
          />
        ) : (
          <SettingRow
            title={t('repository.repository')}
            description={t('repository.repositoryDescription')}
            htmlFor="github-repository"
          >
            {picker}
          </SettingRow>
        )}
      </SettingSection>

      {repository && (
        <>
          <SettingSection
            title={t('repository.mappings')}
            description={t('repository.mappingsDescription')}
          >
            <PathMappings
              // A new key per saved value starts the draft again from it,
              // so what the server kept (empty rows dropped) is what shows.
              key={JSON.stringify(repository.code_mappings)}
              saved={repository.code_mappings}
              disabled={isPending}
              onSave={(code_mappings) => save({ code_mappings })}
            />
          </SettingSection>

          <SettingSection title={t('repository.issues')}>
            <SettingRow
              title={t('repository.autoCreate')}
              description={t('repository.autoCreateDescription')}
              htmlFor="github-auto-create"
            >
              <Switch
                id="github-auto-create"
                checked={repository.auto_create_issues}
                disabled={isPending}
                onCheckedChange={(checked) =>
                  save({ auto_create_issues: checked })
                }
              />
            </SettingRow>
            {repository.auto_create_issues && (
              <SettingRow
                title={t('repository.autoCreateFilters')}
                description={t('repository.autoCreateFiltersDescription')}
              >
                <AutoCreateFilters
                  saved={repository.auto_create_conditions}
                  disabled={isPending}
                  onSave={(auto_create_conditions) =>
                    save({ auto_create_conditions })
                  }
                />
              </SettingRow>
            )}
            <SettingRow
              title={t('repository.fixes')}
              description={t('repository.fixesDescription')}
            >
              <code className="rounded-md bg-surface-chip px-2 py-1 font-mono text-mono text-fg">
                Fixes {projectSlug.toUpperCase()}-123
              </code>
            </SettingRow>
            <SettingRow
              title={t('repository.codeowners')}
              description={t('repository.codeownersDescription')}
            >
              <div className="flex items-center gap-3">
                <Button
                  size="sm"
                  icon={RefreshIcon}
                  disabled={isPending}
                  onClick={() => syncCodeowners.mutate()}
                >
                  {t('repository.codeownersSync')}
                </Button>
                <span className="text-hint text-fg-meta">
                  {repository.codeowners_synced_at
                    ? t('repository.codeownersStatus', {
                        count: repository.codeowners_rules ?? 0,
                        when: format.relativeTime(
                          new Date(repository.codeowners_synced_at),
                        ),
                      })
                    : t('repository.codeownersNever')}
                </span>
              </div>
            </SettingRow>
          </SettingSection>

          <SettingSection
            title={t('template.section')}
            description={t('template.projectDescription')}
          >
            <IssueTemplateFields
              key={JSON.stringify(repository.issue_template)}
              saved={repository.issue_template}
              disabled={isPending}
              onSave={(issue_template) => save({ issue_template })}
            />
          </SettingSection>

          <SettingSection title={t('repository.danger')} destructive>
            <SettingRow
              title={t('repository.disconnect')}
              description={t('repository.disconnectDescription')}
            >
              <Button
                variant="danger"
                disabled={isPending}
                onClick={() => removeRepository.mutate()}
              >
                {t('repository.disconnect')}
              </Button>
            </SettingRow>
          </SettingSection>
        </>
      )}
    </>
  );
}

/** The path mapping rows, edited locally and saved together. */
function PathMappings({
  saved,
  disabled,
  onSave,
}: {
  saved: CodeMapping[];
  disabled: boolean;
  onSave: (mappings: CodeMapping[]) => void;
}) {
  const t = useTranslations('github');
  // A draft of `saved`, not a copy to keep in step: the parent keys this
  // component on `saved`, so a new saved value starts a new draft.
  // react-doctor-disable-next-line react-doctor/no-derived-useState
  const [mappings, setMappings] = useState<CodeMapping[]>(saved);
  const changed = JSON.stringify(mappings) !== JSON.stringify(saved);
  const update = (index: number, change: Partial<CodeMapping>) =>
    setMappings(
      mappings.map((m, i) => (i === index ? { ...m, ...change } : m)),
    );

  return (
    <>
      {mappings.map((mapping, index) => (
        // Rows are controlled inputs with no identity of their own; the
        // position is the row, and removing one re-renders from state.
        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
        <div
          key={index}
          className="flex items-center gap-2 border-b border-border-divider px-5 py-3"
        >
          <Input
            size="sm"
            value={mapping.stack_root}
            placeholder="/app/"
            aria-label={t('repository.stackRoot')}
            className="font-mono text-mono"
            boxClassName="flex-1"
            disabled={disabled}
            onChange={(e) => update(index, { stack_root: e.target.value })}
          />
          <span aria-hidden="true" className="text-fg-meta">
            →
          </span>
          <Input
            size="sm"
            value={mapping.source_root}
            placeholder="src/"
            aria-label={t('repository.sourceRoot')}
            className="font-mono text-mono"
            boxClassName="flex-1"
            disabled={disabled}
            onChange={(e) => update(index, { source_root: e.target.value })}
          />
          <Button
            variant="ghost"
            size="sm"
            icon={CloseIcon}
            aria-label={t('repository.removeMapping')}
            disabled={disabled}
            onClick={() => setMappings(mappings.filter((_, i) => i !== index))}
          />
        </div>
      ))}
      <div className="flex items-center justify-between gap-2 px-5 py-3">
        <Button
          variant="ghost"
          size="sm"
          icon={NewIcon}
          disabled={disabled}
          onClick={() =>
            setMappings([...mappings, { stack_root: '', source_root: '' }])
          }
        >
          {t('repository.addMapping')}
        </Button>
        {changed && (
          <Button
            variant="primary"
            size="sm"
            disabled={disabled}
            onClick={() => onSave(savableMappings(mappings))}
          >
            {t('save')}
          </Button>
        )}
      </div>
    </>
  );
}

/** The automatic issues' filters, edited locally and saved together. */
function AutoCreateFilters({
  saved,
  disabled,
  onSave,
}: {
  saved: AlertRuleResponse['conditions'];
  disabled: boolean;
  onSave: (conditions: AlertRuleResponse['conditions']) => void;
}) {
  const t = useTranslations('github');
  const [filters, setFilters] = useState(() => formFromConditions(saved));
  const conditions = conditionsFromForm({
    alert_type: 'new_issue',
    ...filters,
  });
  const changed =
    JSON.stringify(conditions) !==
    JSON.stringify(
      conditionsFromForm({
        alert_type: 'new_issue',
        ...formFromConditions(saved),
      }),
    );

  return (
    <div className="flex flex-col gap-3">
      <ConditionFilters
        value={filters}
        onChange={setFilters}
        showMinEvents
        disabled={disabled}
      />
      {changed && (
        <div className="flex justify-end">
          <Button
            variant="primary"
            size="sm"
            disabled={disabled}
            onClick={() => onSave(conditions)}
          >
            {t('save')}
          </Button>
        </div>
      )}
    </div>
  );
}
