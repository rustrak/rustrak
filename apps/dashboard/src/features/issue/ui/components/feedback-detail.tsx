import type { AttachmentInfo, FeedbackResponse } from '@rustrak/client';
import {
  ArrowRightIcon,
  AttachmentIcon,
  Avatar,
  BackIcon,
  Button,
  ErrorIcon,
  ExternalLinkIcon,
  MuteIcon,
  ResolveIcon,
  Tag,
  Text,
  TimeIcon,
  UnresolveIcon,
} from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import {
  feedbackSender,
  isWebUrl,
} from '@/features/issue/model/feedback-mailbox';
import { STATUS_TONE } from '@/features/issue/model/status';
import { isInlineImage } from '@/shared/lib/utils';
import { Link } from '@/shared/ui/components/link';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';

/**
 * The reading pane: one feedback in full. Who sent it and the two verbs of
 * triage on top, then what they wrote, what they attached, and where they
 * were -- the page, and the error they were looking at when they wrote.
 */
export function FeedbackDetail({
  projectId,
  feedback,
  isPending,
  onResolve,
  onSpam,
  onBack,
}: {
  projectId: number;
  feedback: FeedbackResponse;
  isPending: boolean;
  onResolve: () => void;
  onSpam: () => void;
  /** Back to the list, where the pane replaces it on a narrow screen. */
  onBack: () => void;
}) {
  const t = useTranslations('projectPages.feedback');
  const format = useFormatter();
  const sender = feedbackSender(feedback);
  const resolved = feedback.status === 'resolved';
  const spam = feedback.status === 'ignored';
  const received = new Date(feedback.created_at);
  const issueHref = `/projects/${projectId}/issues/${feedback.id}`;
  const attachmentHref = (attachment: AttachmentInfo) =>
    `/api/projects/${projectId}/attachments/${attachment.id}`;

  return (
    <article className="@container flex min-h-0 flex-1 flex-col overflow-y-auto">
      <header className="flex flex-col gap-4 border-border-divider border-b py-5 lg:px-6 @xl:flex-row @xl:items-start @xl:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <Button
            variant="ghost"
            size="sm"
            icon={BackIcon}
            aria-label={t('back')}
            className="lg:hidden"
            onClick={onBack}
          />
          <Avatar name={sender || t('anonymous')} size="md" />
          <div className="flex min-w-0 flex-col gap-0.5">
            <div className="flex min-w-0 items-center gap-2">
              <h2 className="truncate text-fg text-title">
                {sender || t('anonymous')}
              </h2>
              {feedback.status !== 'unresolved' ? (
                <Tag variant="soft" tone={STATUS_TONE[feedback.status]}>
                  {t(`mailbox.${resolved ? 'resolved' : 'spam'}`)}
                </Tag>
              ) : null}
            </div>
            <p className="flex min-w-0 flex-wrap items-center gap-x-3 text-fg-subtle text-meta">
              {feedback.contact_email ? (
                <a
                  href={`mailto:${feedback.contact_email}`}
                  className="truncate text-fg-secondary hover:text-fg hover:underline"
                >
                  {feedback.contact_email}
                </a>
              ) : null}
              <time
                dateTime={feedback.created_at}
                title={format.dateTime(received, 'dateTime')}
              >
                {format.relativeTime(received)}
              </time>
            </p>
          </div>
        </div>

        {/* Full-width halves on a phone, where there is no keyboard to hint at. */}
        <TriageButtons
          projectId={projectId}
          resolved={resolved}
          spam={spam}
          isPending={isPending}
          onResolve={onResolve}
          onSpam={onSpam}
        />
      </header>

      <div className="flex flex-col gap-6 py-6 lg:px-6">
        <blockquote className="whitespace-pre-wrap break-words border-border-strong border-l-2 pl-4 text-body text-fg leading-relaxed">
          {feedback.message}
        </blockquote>

        {feedback.attachments.length > 0 ? (
          <section className="flex flex-col gap-2">
            <Text variant="meta" tone="subtle">
              {t('attachments', { count: feedback.attachments.length })}
            </Text>
            <div className="flex flex-wrap gap-3">
              {feedback.attachments.map((attachment) => (
                <a
                  key={attachment.id}
                  href={attachmentHref(attachment)}
                  target="_blank"
                  rel="noreferrer"
                  className="group block max-w-full"
                >
                  {/* The server serves only real images inline; anything else
                      downloads, so it gets a link rather than a broken image. */}
                  {isInlineImage(attachment.content_type) ? (
                    <img
                      src={attachmentHref(attachment)}
                      alt={attachment.filename}
                      loading="lazy"
                      className="max-h-96 max-w-full rounded-lg border border-border object-contain transition-opacity group-hover:opacity-90"
                    />
                  ) : (
                    <span className="inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1.5 text-body text-fg-secondary hover:text-fg">
                      <AttachmentIcon size="sm" aria-hidden="true" />
                      {attachment.filename}
                    </span>
                  )}
                </a>
              ))}
            </div>
          </section>
        ) : null}

        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-3 rounded-lg border border-border bg-panel p-4 text-body">
          {feedback.url ? (
            <Fact label={t('page')}>
              {isWebUrl(feedback.url) ? (
                <a
                  href={feedback.url}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex max-w-full items-center gap-1.5 font-mono text-fg-secondary text-mono-sm hover:text-fg hover:underline"
                >
                  <span className="truncate">{feedback.url}</span>
                  <ExternalLinkIcon size="sm" aria-hidden="true" />
                </a>
              ) : (
                <span className="block truncate font-mono text-fg-secondary text-mono-sm">
                  {feedback.url}
                </span>
              )}
            </Fact>
          ) : null}
          <Fact label={t('linkedError')}>
            {feedback.associated_event_id ? (
              <Link
                href={`/projects/${projectId}/events/sentry/${feedback.associated_event_id}`}
                className="inline-flex items-center gap-1.5 text-sev-error-fg hover:underline"
              >
                <ErrorIcon size="sm" aria-hidden="true" />
                {t('viewError')}
              </Link>
            ) : (
              <span className="text-fg-subtle">{t('noLinkedError')}</span>
            )}
          </Fact>
          <Fact label={t('received')}>
            <span className="inline-flex items-center gap-1.5 text-fg-secondary">
              <TimeIcon size="sm" aria-hidden="true" />
              {format.dateTime(received, 'dateTime')}
            </span>
          </Fact>
        </dl>

        <Link
          href={issueHref}
          className="inline-flex items-center gap-1.5 self-start text-body text-fg-secondary hover:text-fg"
        >
          {t('openIssue')}
          <ArrowRightIcon size="sm" aria-hidden="true" />
        </Link>
      </div>
    </article>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-fg-subtle">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </>
  );
}

/** Resolve and spam: only for a reader who may triage (`issues:update`). */
function TriageButtons({
  projectId,
  resolved,
  spam,
  isPending,
  onResolve,
  onSpam,
}: {
  projectId: number;
  resolved: boolean;
  spam: boolean;
  isPending: boolean;
  onResolve: () => void;
  onSpam: () => void;
}) {
  const tIssues = useTranslations('issues');
  if (!usePermissionsLater().can('issues', 'update', projectId)) return null;
  return (
    <div className="grid shrink-0 grid-cols-2 gap-2 max-lg:[&_kbd]:hidden @xl:flex @xl:items-center">
      <Button
        variant={resolved ? 'secondary' : 'primary'}
        icon={resolved ? UnresolveIcon : ResolveIcon}
        shortcut="R"
        disabled={isPending}
        onClick={onResolve}
      >
        {tIssues(resolved ? 'actions.unresolve' : 'actions.resolve')}
      </Button>
      <Button
        icon={MuteIcon}
        shortcut="E"
        disabled={isPending}
        onClick={onSpam}
      >
        {tIssues(spam ? 'actions.moveToInbox' : 'actions.markSpam')}
      </Button>
    </div>
  );
}
