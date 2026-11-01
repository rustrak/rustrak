import type { GitHubStatus } from '@rustrak/client';
import {
  Button,
  CommitIcon,
  ExternalLinkIcon,
  Field,
  FieldHint,
  FieldLabel,
  type IconComponent,
  Input,
  IssuesIcon,
  LinkIcon,
  OkIcon,
  Tag,
  TeamIcon,
  WarningIcon,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { useState } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { githubMutations } from '@/features/github/api/mutations';
import { unreachableFromGitHub } from '@/features/github/lib/webhook-url';
import { IssueTemplateFields } from '@/features/github/ui/components/issue-template-fields';
import { cn } from '@/shared/lib/utils';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { Link } from '@/shared/ui/components/link';
import {
  SettingRow,
  SettingSection,
  SettingsListItem,
} from '@/shared/ui/components/setting-row';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/shared/ui/components/shadcn/alert-dialog';

/**
 * Posts the manifest to GitHub the only way GitHub accepts it: a form the
 * browser submits, so GitHub can show its own "Create GitHub App" page.
 */
function submitManifest(actionUrl: string, manifest: string) {
  const form = document.createElement('form');
  form.method = 'post';
  form.action = actionUrl;
  const field = document.createElement('input');
  field.type = 'hidden';
  field.name = 'manifest';
  field.value = manifest;
  form.appendChild(field);
  document.body.appendChild(form);
  form.submit();
}

/**
 * The GitHub integration's own page: the app this instance runs as, the
 * accounts it is installed on, whether GitHub can reach it, the reader's own
 * login, and the way out. Everything about GitHub is configured here and
 * nowhere else.
 *
 * Until the app is installed somewhere, the page is the setup: the steps to
 * get there, which one you are on, and what it is all for.
 */
export function GitHubIntegration({
  status,
  login,
}: {
  status: GitHubStatus;
  /** The reader's own GitHub login row, rendered by the route. */
  login?: ReactNode;
}) {
  const t = useTranslations('github');
  const [confirming, setConfirming] = useState(false);
  const { mutate, isPending } = useMutation({
    ...githubMutations.disconnect(),
    onSuccess: () => setConfirming(false),
    onError: (error) =>
      toast.error(t('connection.disconnectFailed'), {
        description: error.message,
      }),
  });
  const { app, installations } = status;

  const disconnect = () => mutate();

  // Said before anything is created: an app GitHub cannot deliver to would
  // half work, and the fix is on this server, not on GitHub.
  const unreachable =
    !app?.last_webhook_at && unreachableFromGitHub(status.webhook_url);

  return (
    <>
      {unreachable ? (
        <div className="flex gap-3 rounded-2xl border border-sev-warning bg-sev-warning-surface px-5 py-4">
          <WarningIcon className="mt-0.5 shrink-0 text-sev-warning-fg" />
          <p className="text-meta text-fg">
            {t('connection.unreachable', { url: status.webhook_url })}
          </p>
        </div>
      ) : null}

      {app && installations.length > 0 ? (
        <Connected status={status} login={login} />
      ) : (
        <Setup status={status} />
      )}

      {app ? (
        <SettingSection title={t('repository.danger')} destructive>
          <SettingRow
            title={t('connection.disconnect')}
            description={t('connection.disconnectDescription')}
          >
            <Button
              variant="danger"
              disabled={isPending}
              onClick={() => setConfirming(true)}
            >
              {t('connection.disconnect')}
            </Button>
          </SettingRow>
        </SettingSection>
      ) : null}

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t('connection.disconnectTitle')}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t('connection.disconnectDescription')}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={isPending}>
              {t('cancel')}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={disconnect}
              disabled={isPending}
              className="bg-danger text-white hover:bg-danger-fg"
            >
              {t('connection.disconnect')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

/**
 * Getting from nothing to an installed app: three steps, the one you are on
 * open, the ones behind you ticked, the one after waiting. The last is done
 * per project, so here it only says where.
 */
function Setup({ status }: { status: GitHubStatus }) {
  const t = useTranslations('github');
  const [organization, setOrganization] = useState('');
  const { mutate, isPending } = useMutation({
    ...githubMutations.manifest(),
    onSuccess: (data) => submitManifest(data.action_url, data.manifest),
    onError: (error) =>
      toast.error(t('connection.connectFailed'), {
        description: error.message,
      }),
  });
  const { app } = status;

  const connect = () => mutate(organization.trim() || undefined);

  return (
    <>
      <SettingSection
        title={t('setup.title')}
        description={t('connection.intro')}
      >
        <ol>
          <Step
            number={1}
            state={app ? 'done' : 'current'}
            title={t('setup.createTitle')}
            description={
              app ? (
                <a
                  href={app.html_url}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 hover:text-fg hover:underline"
                >
                  {t('setup.created', { slug: app.slug })}
                  <ExternalLinkIcon size="sm" />
                </a>
              ) : (
                t('setup.createDescription')
              )
            }
          >
            <form
              className="flex max-w-md flex-col items-start gap-4"
              onSubmit={(e) => {
                e.preventDefault();
                connect();
              }}
            >
              <Field className="w-full">
                <FieldLabel>{t('setup.organization')}</FieldLabel>
                <Input
                  value={organization}
                  onChange={(e) => setOrganization(e.target.value)}
                  placeholder={t('connection.organizationPlaceholder')}
                  disabled={isPending}
                />
                <FieldHint>{t('setup.organizationHint')}</FieldHint>
              </Field>
              <Button type="submit" variant="primary" loading={isPending}>
                <GitHubMark className="size-4" />
                {t('connection.connect')}
              </Button>
            </form>
          </Step>
          <Step
            number={2}
            state={app ? 'current' : 'next'}
            title={t('setup.installTitle')}
            description={t('setup.installDescription')}
          >
            {app ? (
              <Button
                variant="primary"
                icon={ExternalLinkIcon}
                render={<Link href={app.install_url} />}
              >
                {t('connection.install')}
              </Button>
            ) : null}
          </Step>
          <Step
            number={3}
            state="next"
            title={t('setup.linkTitle')}
            description={t('setup.linkDescription')}
          />
        </ol>
      </SettingSection>

      <SettingSection title={t('setup.featuresTitle')}>
        <div className="grid gap-px bg-border-divider sm:grid-cols-2">
          <Feature
            icon={LinkIcon}
            title={t('setup.features.code')}
            description={t('setup.features.codeDescription')}
          />
          <Feature
            icon={CommitIcon}
            title={t('setup.features.commits')}
            description={t('setup.features.commitsDescription')}
          />
          <Feature
            icon={IssuesIcon}
            title={t('setup.features.issues')}
            description={t('setup.features.issuesDescription')}
          />
          <Feature
            icon={TeamIcon}
            title={t('setup.features.owners')}
            description={t('setup.features.ownersDescription')}
          />
        </div>
      </SettingSection>
    </>
  );
}

type StepState = 'done' | 'current' | 'next';

/** One step of the setup. Only the current one shows its controls. */
function Step({
  number,
  state,
  title,
  description,
  children,
}: {
  number: number;
  state: StepState;
  title: string;
  description: ReactNode;
  children?: ReactNode;
}) {
  return (
    <li
      aria-current={state === 'current' ? 'step' : undefined}
      className="flex gap-4 border-b border-border-divider px-5 py-5 last:border-b-0"
    >
      <span
        className={cn(
          'flex size-7 shrink-0 items-center justify-center rounded-pill text-meta font-semibold',
          state === 'done' && 'bg-success-surface text-success-fg',
          state === 'current' && 'bg-surface-brand text-fg-on-brand',
          state === 'next' && 'border border-border text-fg-subtle',
        )}
      >
        {state === 'done' ? <OkIcon size="sm" /> : number}
      </span>
      <div className="min-w-0 flex-1 pt-0.5">
        <p
          className={cn(
            'text-value font-medium',
            state === 'next' ? 'text-fg-subtle' : 'text-fg',
          )}
        >
          {title}
        </p>
        <div className="mt-1 text-hint text-fg-subtle">{description}</div>
        {state === 'current' && children ? (
          <div className="mt-4">{children}</div>
        ) : null}
      </div>
    </li>
  );
}

/** One thing the integration adds, for the reader deciding whether to. */
function Feature({
  icon: Icon,
  title,
  description,
}: {
  icon: IconComponent;
  title: string;
  description: string;
}) {
  return (
    <div className="flex gap-3 bg-surface px-5 py-4">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border-subtle text-fg-secondary">
        <Icon size="sm" />
      </span>
      <div className="min-w-0">
        <p className="text-value font-medium text-fg">{title}</p>
        <p className="mt-0.5 text-hint text-fg-subtle">{description}</p>
      </div>
    </div>
  );
}

/** The app, where it is installed, whether GitHub reaches us, and you. */
function Connected({
  status,
  login,
}: {
  status: GitHubStatus;
  login?: ReactNode;
}) {
  const t = useTranslations('github');
  const format = useFormatter();
  const { installations, webhook_url } = status;
  // Only rendered once the app exists.
  const app = status.app as NonNullable<GitHubStatus['app']>;

  return (
    <>
      <SettingSection
        title={t('section.title')}
        description={t('connection.description')}
      >
        <SettingRow title={t('connection.app')}>
          <a
            href={app.html_url}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 font-mono text-mono text-fg hover:underline"
          >
            {app.slug}
            <ExternalLinkIcon size="sm" />
          </a>
        </SettingRow>
        <SettingRow title={t('connection.webhooks')}>
          {app.last_webhook_at ? (
            <span className="inline-flex items-center gap-2 text-meta text-fg-meta">
              <span className="size-2 rounded-pill bg-success" />
              {t('connection.lastWebhook', {
                when: format.relativeTime(new Date(app.last_webhook_at)),
              })}
            </span>
          ) : (
            <span className="inline-flex items-center gap-2 text-meta text-fg-meta">
              <span
                className={cn(
                  'size-2 rounded-pill',
                  unreachableFromGitHub(webhook_url)
                    ? 'bg-sev-warning'
                    : 'bg-fg-ghost',
                )}
              />
              {t('connection.noWebhook')}
            </span>
          )}
        </SettingRow>
      </SettingSection>

      <SettingSection
        title={t('connection.accounts')}
        actions={
          <Button
            variant="secondary"
            icon={ExternalLinkIcon}
            render={<Link href={app.install_url} />}
          >
            {t('connection.install')}
          </Button>
        }
      >
        {installations.map((installation) => (
          <SettingsListItem
            key={installation.id}
            leading={
              <span className="flex size-8 items-center justify-center rounded-lg border border-border-subtle text-fg-secondary">
                <GitHubMark className="size-4" />
              </span>
            }
            title={installation.account_login}
            description={installation.account_type}
            meta={
              installation.suspended ? (
                <Tag variant="soft" tone="warning">
                  {t('connection.suspended')}
                </Tag>
              ) : (
                <Tag variant="soft" tone="brand">
                  {t('setup.active')}
                </Tag>
              )
            }
          />
        ))}
      </SettingSection>

      <InstanceIssueTemplate saved={status.issue_template} />

      {login}
    </>
  );
}

/** The template every project's GitHub issues start with. */
function InstanceIssueTemplate({
  saved,
}: {
  saved: GitHubStatus['issue_template'];
}) {
  const t = useTranslations('github');
  const { mutate, isPending } = useMutation({
    ...githubMutations.setIssueTemplate(),
    onError: (error) =>
      toast.error(t('template.saveFailed'), { description: error.message }),
  });
  return (
    <SettingSection
      title={t('template.section')}
      description={t('template.instanceDescription')}
    >
      <IssueTemplateFields
        key={JSON.stringify(saved)}
        saved={saved}
        disabled={isPending}
        onSave={(template) => mutate(template)}
      />
    </SettingSection>
  );
}
