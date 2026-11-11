import type { Schedule } from '@rustrak/client';
import {
  Button,
  CopyIcon,
  SelectedIcon,
  Tab,
  TabList,
  Tabs,
} from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  CRON_GUIDES,
  type CronGuideId,
  cronGuide,
} from '@/features/monitor/lib/setup';
import { CodeBlock } from '@/shared/ui/components/code-block';
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
  const highlighter = useSyntaxHighlighter();
  const [tab, setTab] = useState<CronGuideId>('curl');
  const { isCopied, copy } = useCopyFlags();
  const { code, language } = cronGuide(tab, { dsn, slug, schedule });

  return (
    <div className="overflow-hidden rounded-lg border border-border bg-surface">
      <div className="flex items-center justify-between gap-2 border-border-divider border-b ps-3 pe-2">
        <Tabs
          value={tab}
          onValueChange={(value) => setTab(value as CronGuideId)}
        >
          <TabList className="border-b-0">
            {CRON_GUIDES.map((id) => (
              <Tab key={id} value={id}>
                {t(`guides.${id}`)}
              </Tab>
            ))}
          </TabList>
        </Tabs>
        <Button
          variant="ghost"
          size="sm"
          icon={isCopied(tab) ? SelectedIcon : CopyIcon}
          onClick={() => copy(tab, code, t(`guides.${tab}`))}
        >
          {t(isCopied(tab) ? 'setup.copied' : 'setup.copy')}
        </Button>
      </div>
      <div className="overflow-x-auto">
        <CodeBlock code={code} language={language} highlighter={highlighter} />
      </div>
    </div>
  );
}
