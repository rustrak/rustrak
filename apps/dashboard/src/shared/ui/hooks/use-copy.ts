import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { copyToClipboard } from '@/shared/lib/clipboard';

/**
 * Copies text, ticks `copied` for two seconds, and toasts `done` if given.
 *
 * A clipboard that refuses says so: `navigator.clipboard` is unavailable over
 * plain HTTP, which is how a self-hosted install is often reached, and a
 * button that quietly does nothing there is worse than one that explains.
 * `hint` replaces the generic advice in that toast.
 */
export function useCopy(hint?: string) {
  const t = useTranslations('common');
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = async (text: string, done?: string): Promise<boolean> => {
    if (!(await copyToClipboard(text))) {
      toast.info(t('clipboardUnavailable'), {
        description: hint ?? t('clipboardUnavailableHint'),
      });
      return false;
    }
    if (done) toast.success(done);
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 2000);
    return true;
  };

  return { copied, copy };
}
