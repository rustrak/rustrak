import type { AttachmentInfo } from '@rustrak/client';
import {
  AttachmentIcon,
  ExternalLinkIcon,
  MailIcon,
  MemberIcon,
} from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import type { Feedback } from '@/features/event/lib/feedback';
import { Link } from '@/shared/ui/components/link';

/**
 * What a user wrote, who they said they are, and where they were: the body
 * of a feedback, which the rest of the event page only decorates.
 */
export function FeedbackSection({
  feedback,
  errorHref,
  attachments,
  attachmentHref,
}: {
  feedback: Feedback;
  /** Where the error the user was reporting on lives, if they were. */
  errorHref: string | null;
  /** Files sent with it, such as a screenshot. */
  attachments: AttachmentInfo[];
  attachmentHref: (attachment: AttachmentInfo) => string;
}) {
  const t = useTranslations('projectPages.event.feedback');
  return (
    <div className="space-y-4">
      <p className="whitespace-pre-wrap break-words text-sm leading-relaxed">
        {feedback.message}
      </p>

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-sm">
        <dt className="flex items-center gap-1.5 text-fg-subtle">
          <MemberIcon className="size-3.5" />
          {t('from')}
        </dt>
        <dd className="min-w-0 truncate">{feedback.name || t('anonymous')}</dd>

        {feedback.contactEmail && (
          <>
            <dt className="flex items-center gap-1.5 text-fg-subtle">
              <MailIcon className="size-3.5" />
              {t('email')}
            </dt>
            <dd className="min-w-0 truncate">
              <a
                href={`mailto:${feedback.contactEmail}`}
                className="text-fg-brand hover:underline"
              >
                {feedback.contactEmail}
              </a>
            </dd>
          </>
        )}

        {feedback.url && (
          <>
            <dt className="flex items-center gap-1.5 text-fg-subtle">
              <ExternalLinkIcon className="size-3.5" />
              {t('page')}
            </dt>
            <dd className="min-w-0 truncate font-mono text-xs leading-5">
              {feedback.url}
            </dd>
          </>
        )}
      </dl>

      {attachments.length > 0 && (
        <div className="flex flex-wrap gap-3">
          {attachments.map((attachment) => (
            <a
              key={attachment.id}
              href={attachmentHref(attachment)}
              target="_blank"
              rel="noreferrer"
              className="group block max-w-full"
            >
              {/* The server serves only real images inline; anything else
                  downloads, so it gets a link rather than a broken image. */}
              {isImage(attachment.content_type) ? (
                <img
                  src={attachmentHref(attachment)}
                  alt={attachment.filename}
                  loading="lazy"
                  className="max-h-80 max-w-full rounded-md border object-contain transition-opacity group-hover:opacity-90"
                />
              ) : (
                <span className="inline-flex items-center gap-1.5 text-sm text-fg-brand hover:underline">
                  <AttachmentIcon className="size-3.5" />
                  {attachment.filename}
                </span>
              )}
            </a>
          ))}
        </div>
      )}

      {errorHref && (
        <Link
          href={errorHref}
          className="inline-flex text-sm font-medium text-fg-brand hover:underline"
        >
          {t('viewError')}
        </Link>
      )}
    </div>
  );
}

/** The formats the server serves inline. */
function isImage(contentType: string) {
  return ['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(
    contentType,
  );
}
