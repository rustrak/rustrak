import { Button, CopyIcon, Menu, ResolveIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { useCopy } from '@/shared/ui/hooks/use-copy';

interface CopyAsFormat {
  label: string;
  value: string;
}

interface CopyAsDropdownProps {
  formats: CopyAsFormat[];
}

/** "Copy as ..." dropdown for section headers (Stack Trace, Breadcrumbs). */
export function CopyAsDropdown({ formats }: CopyAsDropdownProps) {
  const t = useTranslations('common');
  const { copied, copy } = useCopy();

  return (
    <Menu
      align="end"
      actions={formats.map((format) => ({
        id: format.label,
        label: format.label,
        onSelect: () => copy(format.value),
      }))}
      trigger={
        <Button
          variant="ghost"
          size="sm"
          icon={copied ? ResolveIcon : CopyIcon}
          menu
        >
          {t('copyAs')}
        </Button>
      }
    />
  );
}
