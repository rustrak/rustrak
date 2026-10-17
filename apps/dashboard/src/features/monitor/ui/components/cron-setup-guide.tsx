import type { Schedule } from '@rustrak/client';
import { Check, Copy } from 'lucide-react';
import { useTheme } from 'next-themes';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  CRON_GUIDES,
  type CronGuideId,
  cronGuide,
} from '@/features/monitor/lib/setup';
import { CodeBlock } from '@/shared/ui/components/code-block';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Tabs,
  TabsList,
  TabsTrigger,
} from '@/shared/ui/components/shadcn/tabs';
import { useCopyFlags } from '@/shared/ui/hooks/use-copy-flags';
import { useSyntaxHighlighter } from '@/shared/ui/hooks/use-syntax-highlighter';

/**
 * How to make a job check in, one tab per tool, with the DSN and slug filled
 * in. Given a schedule, the snippet also creates the monitor on its first
 * run, which is how most monitors come to exist.
 */
export function CronSetupGuide({
  dsn,
  slug,
  schedule,
}: {
  dsn: string;
  slug: string;
  schedule?: Schedule;
}) {
  const t = useTranslations('crons');
  const { resolvedTheme } = useTheme();
  const highlighter = useSyntaxHighlighter(resolvedTheme === 'dark');
  const [tab, setTab] = useState<CronGuideId>('curl');
  const { isCopied, copy } = useCopyFlags({
    unavailable: t('setup.copyUnavailable'),
    hint: (label) => t('setup.copyUnavailableHint', { label }),
  });
  const { code, language } = cronGuide(tab, { dsn, slug, schedule });

  return (
    <div className="overflow-hidden rounded-lg border">
      <div className="flex items-center justify-between gap-2 border-b bg-muted/40 px-2 py-1.5">
        <Tabs
          value={tab}
          onValueChange={(value) => setTab(value as CronGuideId)}
        >
          <TabsList className="h-8 bg-transparent">
            {CRON_GUIDES.map((id) => (
              <TabsTrigger key={id} value={id} className="h-7 px-2.5 text-xs">
                {t(`guides.${id}`)}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <Button
          variant="ghost"
          size="xs"
          onClick={() => copy(tab, code, t(`guides.${tab}`))}
        >
          {isCopied(tab) ? (
            <>
              <Check className="text-primary" />
              {t('setup.copied')}
            </>
          ) : (
            <>
              <Copy />
              {t('setup.copy')}
            </>
          )}
        </Button>
      </div>
      <div className="overflow-x-auto">
        <CodeBlock code={code} language={language} highlighter={highlighter} />
      </div>
    </div>
  );
}
