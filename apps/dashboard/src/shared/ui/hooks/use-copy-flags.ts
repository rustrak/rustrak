import { useState } from 'react';
import { toast } from 'sonner';
import { copyToClipboard } from '@/shared/lib/clipboard';

export interface CopyFlags {
  /** Whether this target was copied within the last couple of seconds. */
  isCopied: (target: string) => boolean;
  copy: (target: string, value: string, label: string) => Promise<void>;
}

interface CopyMessages {
  /** Shown when the clipboard is unavailable, with the hint below it. */
  unavailable: string;
  hint: (label: string) => string;
}

/**
 * "Copied" ticks, one per copyable thing on a page.
 *
 * A set rather than one "most recently copied" value: the ticks are
 * independent, so copying the install command must not silently un-tick the
 * DSN the reader copied a moment earlier.
 *
 * A clipboard that refuses says so. `navigator.clipboard` is unavailable over
 * plain HTTP, which is exactly how a self-hosted install is often reached, and
 * a button that quietly does nothing there is worse than one that explains.
 */
export function useCopyFlags(messages: CopyMessages): CopyFlags {
  const [copied, setCopied] = useState<ReadonlySet<string>>(new Set());

  const copy = async (target: string, value: string, label: string) => {
    if (!(await copyToClipboard(value))) {
      toast.info(messages.unavailable, { description: messages.hint(label) });
      return;
    }

    setCopied((prev) => new Set(prev).add(target));
    setTimeout(() => {
      setCopied((prev) => {
        const next = new Set(prev);
        next.delete(target);
        return next;
      });
    }, 2000);
  };

  return { isCopied: (target) => copied.has(target), copy };
}
