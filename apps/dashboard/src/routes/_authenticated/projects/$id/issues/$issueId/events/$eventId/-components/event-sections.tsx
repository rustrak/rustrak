import type { AttachmentInfo, EventDetailResponse } from '@rustrak/client';
import { useInfiniteQuery } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import type { readEventPayload } from '@/features/event/lib/event-payload';
import { readFeedback } from '@/features/event/lib/feedback';
import { formatStackTraceAsText } from '@/features/event/lib/format-stack-trace';
import { AttachmentsSection } from '@/features/event/ui/components/attachments-section';
import { Breadcrumbs } from '@/features/event/ui/components/breadcrumbs';
import { EventContext } from '@/features/event/ui/components/event-context';
import { EventDetails } from '@/features/event/ui/components/event-details';
import { EventHighlights } from '@/features/event/ui/components/event-highlights';
import { EventTags } from '@/features/event/ui/components/event-tags';
import { FeedbackSection } from '@/features/event/ui/components/feedback-section';
import { RawJson } from '@/features/event/ui/components/raw-json';
import type { FrameSourceUrl } from '@/features/event/ui/components/stack-frame-item';
import { StackTrace } from '@/features/event/ui/components/stack-trace';
import { ThreadsSection } from '@/features/event/ui/components/threads-section';
import { issueQueries } from '@/features/issue/api/queries';
import { UserFeedback } from '@/features/issue/ui/components/user-feedback';
import { CopyAsDropdown } from '@/shared/ui/components/copy-as-dropdown';
import { Section } from './collapsible-section';

type EventPayload = ReturnType<typeof readEventPayload>;

/**
 * The stacked detail sections of an event, in reading order.
 *
 * Every one of them is conditional on the payload actually carrying that kind
 * of data, which is why it takes the whole parsed payload rather than a
 * flattened set of props: the `has` flags and the values they guard have to
 * come from the same parse or they can disagree.
 */
export function EventSections({
  projectId,
  issueId,
  attachments,
  event,
  payload,
  eventData,
  sourceUrl,
}: {
  projectId: number;
  issueId: string;
  /** Files sent with the event: inside a feedback, or a section of their own. */
  attachments: AttachmentInfo[];
  event: EventDetailResponse;
  payload: EventPayload;
  eventData: Record<string, unknown>;
  sourceUrl?: FrameSourceUrl;
}) {
  const t = useTranslations('projectPages');
  const {
    exception,
    breadcrumbs,
    threads,
    contexts,
    modules,
    user,
    tags: safeTags,
    has,
  } = payload;
  const feedback = readFeedback(eventData);
  const attachmentHref = (attachment: AttachmentInfo) =>
    `/api/projects/${projectId}/attachments/${attachment.id}`;
  // The feedback users sent about the issue, which the loader started.
  const { data: userReports } = useInfiniteQuery(
    issueQueries.userReports(projectId, issueId),
  );

  return (
    <div className="overflow-hidden rounded-xl border border-border-subtle bg-surface">
      {feedback && (
        <Section id="feedback" title={t('event.sectionFeedback')}>
          <FeedbackSection
            feedback={feedback}
            errorHref={
              feedback.associatedEventId
                ? `/projects/${projectId}/events/sentry/${feedback.associatedEventId}`
                : null
            }
            attachments={attachments}
            attachmentHref={attachmentHref}
          />
        </Section>
      )}

      {(userReports?.pages[0]?.items.length ?? 0) > 0 && (
        <Section id="user-feedback" title={t('event.sectionUserFeedback')}>
          <UserFeedback projectId={projectId} issueId={issueId} />
        </Section>
      )}

      <Section id="highlights" title={t('event.sectionHighlights')}>
        <EventHighlights
          event={event}
          tags={safeTags}
          contexts={contexts}
          eventData={eventData}
        />
      </Section>

      {has.stackTrace && (
        <Section
          id="stacktrace"
          title={t('event.sectionStackTrace')}
          actions={
            // ThreadsSection owns its own copy control: the active
            // thread/exception pairing is its state.
            has.threads ? undefined : (
              <CopyAsDropdown
                formats={[
                  {
                    label: t('event.formatPlainText'),
                    value: formatStackTraceAsText(exception, event.platform),
                  },
                  {
                    label: t('event.formatJson'),
                    value: JSON.stringify(exception, null, 2),
                  },
                ]}
              />
            )
          }
        >
          {has.threads ? (
            <ThreadsSection
              threads={threads}
              exception={exception}
              platform={event.platform}
              sourceUrl={sourceUrl}
            />
          ) : (
            <StackTrace
              exception={exception}
              platform={event.platform}
              sourceUrl={sourceUrl}
            />
          )}
        </Section>
      )}

      {has.breadcrumbs && (
        <Section
          id="breadcrumbs"
          title={t('event.sectionBreadcrumbs')}
          actions={
            <CopyAsDropdown
              formats={[
                {
                  label: t('event.formatJson'),
                  value: JSON.stringify(breadcrumbs, null, 2),
                },
              ]}
            />
          }
        >
          <Breadcrumbs breadcrumbs={breadcrumbs} />
        </Section>
      )}

      {has.tags && (
        <Section id="tags" title={t('event.sectionTags')}>
          <EventTags tags={safeTags} />
        </Section>
      )}

      {(has.contexts || has.modules || has.user) && (
        <Section id="context" title={t('event.sectionContext')}>
          <EventContext contexts={contexts} modules={modules} user={user} />
        </Section>
      )}

      {!feedback && attachments.length > 0 && (
        <Section id="attachments" title={t('event.sectionAttachments')}>
          <AttachmentsSection
            attachments={attachments}
            attachmentHref={attachmentHref}
          />
        </Section>
      )}

      <Section id="details" title={t('event.sectionDetails')}>
        <EventDetails event={event} />
      </Section>

      <Section id="raw" title={t('event.sectionRawJson')} defaultOpen={false}>
        <RawJson data={eventData} />
      </Section>
    </div>
  );
}
