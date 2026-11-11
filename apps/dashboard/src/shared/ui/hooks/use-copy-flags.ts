import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { copyToClipboard } from '@/shared/lib/clipboard';

export interface CopyFlags {
  /** Whether this target was copied within the last couple of seconds. */
  isCopied: (target: string) => boolean;
  /** Copies `value`; `label` names it in the toast if the clipboard refuses. */
  copy: (target: string, value: string, label: string) => Promise<void>;
}

/**
 * `useCopy` for a page with several copyable things: one "copied" tick per
 * target, independent, so copying the install command does not un-tick the
 * DSN copied a moment earlier.
 */
export function useCopyFlags(): CopyFlags {
  const t = useTranslations('common');
  const [copied, setCopied] = useState<ReadonlySet<string>>(new Set());
  const timers = useRef(new Set<ReturnType<typeof setTimeout>>());
  useEffect(() => {
    const pending = timers.current;
    return () => {
      for (const timer of pending) clearTimeout(timer);
    };
  }, []);

  const copy = async (target: string, value: string, label: string) => {
    if (!(await copyToClipboard(value))) {
      toast.info(t('clipboardUnavailable'), {
        description: t('clipboardLabelHint', { label }),
      });
      return;
    }

    setCopied((prev) => new Set(prev).add(target));
    const timer = setTimeout(() => {
      timers.current.delete(timer);
      setCopied((prev) => {
        const next = new Set(prev);
        next.delete(target);
        return next;
      });
    }, 2000);
    timers.current.add(timer);
  };

  return { isCopied: (target) => copied.has(target), copy };
}
