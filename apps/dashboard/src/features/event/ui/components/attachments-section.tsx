import type { AttachmentInfo } from '@rustrak/client';
import { AttachmentIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { formatBytes } from '@/shared/lib/utils';

/** The files sent with an event: name, size and a download link each. */
export function AttachmentsSection({
  attachments,
  attachmentHref,
}: {
  attachments: AttachmentInfo[];
  attachmentHref: (attachment: AttachmentInfo) => string;
}) {
  const t = useTranslations('projectPages.event');
  return (
    <ul className="divide-y divide-border-subtle text-sm">
      {attachments.map((attachment) => (
        <li
          key={attachment.id}
          className="flex items-center gap-3 py-2 first:pt-0 last:pb-0"
        >
          <AttachmentIcon className="size-3.5 shrink-0 text-fg-subtle" />
          <span className="min-w-0 flex-1 truncate font-mono text-xs leading-5">
            {attachment.filename}
          </span>
          <span className="shrink-0 tabular-nums text-fg-subtle">
            {formatBytes(attachment.size)}
          </span>
          <a
            href={attachmentHref(attachment)}
            download={attachment.filename}
            className="shrink-0 font-medium text-fg-brand hover:underline"
          >
            {t('attachmentDownload')}
          </a>
        </li>
      ))}
    </ul>
  );
}
