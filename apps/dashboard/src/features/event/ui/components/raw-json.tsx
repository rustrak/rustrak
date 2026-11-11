import { Button, CopyIcon, SelectedIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { useCopy } from '@/shared/ui/hooks/use-copy';

interface RawJsonProps {
  data: Record<string, unknown>;
}

export function RawJson({ data }: RawJsonProps) {
  const t = useTranslations('events');
  const { copied, copy } = useCopy(t('clipboardJsonHint'));
  const jsonString = JSON.stringify(data, null, 2);

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <Button
          variant="secondary"
          size="sm"
          icon={copied ? SelectedIcon : CopyIcon}
          onClick={() => copy(jsonString)}
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
