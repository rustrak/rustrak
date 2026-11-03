import { Button, CopyIcon, SelectedIcon } from '@rustrak/ui';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { copyToClipboard } from '@/shared/lib/clipboard';

interface RawJsonProps {
  data: Record<string, unknown>;
}

export function RawJson({ data }: RawJsonProps) {
  const t = useTranslations('events');
  const common = useTranslations('common');
  const [copied, setCopied] = useState(false);
  const jsonString = JSON.stringify(data, null, 2);

  const handleCopy = async () => {
    if (!(await copyToClipboard(jsonString))) {
      toast.info(common('clipboardUnavailable'), {
        description: t('clipboardJsonHint'),
      });
      return;
    }

    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <Button
          variant="secondary"
          size="sm"
          icon={copied ? SelectedIcon : CopyIcon}
          onClick={handleCopy}
        >
          {copied ? t('rawJson.copied') : t('rawJson.copyJson')}
        </Button>
      </div>

      <div className="max-h-150 overflow-auto rounded-lg border border-border-subtle bg-panel p-4">
        <pre className="whitespace-pre font-mono text-mono-sm text-fg-secondary">
          {jsonString}
        </pre>
      </div>
    </div>
  );
}
