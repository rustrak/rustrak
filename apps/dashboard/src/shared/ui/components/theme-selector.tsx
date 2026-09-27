import { Monitor, Moon, Sun } from 'lucide-react';
import { useTheme } from 'next-themes';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';

export function ThemeSelector() {
  const { theme, setTheme } = useTheme();
  const t = useTranslations('theme');

  const themes = [
    { value: 'light', label: t('light'), icon: Sun },
    { value: 'dark', label: t('dark'), icon: Moon },
    { value: 'system', label: t('system'), icon: Monitor },
  ] as const;

  return (
    <div className="flex gap-3">
      {themes.map((t) => {
        const Icon = t.icon;
        const isActive = theme === t.value;

        return (
          <button
            key={t.value}
            type="button"
            onClick={() => setTheme(t.value)}
            className={cn(
              'flex flex-1 flex-col items-center justify-center gap-2 h-20 md:h-24 rounded-lg border transition-colors cursor-pointer',
              isActive
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-border hover:border-primary/50 text-muted-foreground hover:text-foreground',
            )}
          >
            <Icon className="size-6" />
            <span className="text-xs font-medium">{t.label}</span>
          </button>
        );
      })}
    </div>
  );
}
