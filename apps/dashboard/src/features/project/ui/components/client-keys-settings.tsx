import type { Project } from '@rustrak/client';
import { Check, Copy } from 'lucide-react';
import { useTheme } from 'next-themes';
import { useTranslations } from 'use-intl';
import { resolveSetupSnippet } from '@/features/project/lib/setup-snippet';
import { useCopyFlags } from '@/features/project/ui/hooks/use-copy-flags';
import { platformLabel } from '@/shared/config/platforms';
import { CodeHighlighter } from '@/shared/ui/components/code-highlighter';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { Button } from '@/shared/ui/components/shadcn/button';

interface ClientKeysSettingsProps {
  project: Project;
}

export function ClientKeysSettings({ project }: ClientKeysSettingsProps) {
  const t = useTranslations('projects');
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === 'dark';
  const { isCopied, copy } = useCopyFlags({
    unavailable: t('copyUnavailable'),
    hint: (label) => t('copyUnavailableHint', { label }),
  });

  const { snippet, docsUrl, codeExample, codeLanguage } = resolveSetupSnippet(
    project.platform,
    project.dsn,
  );

  const exampleTitle = snippet
    ? t('clientKeys.examplePlatform', {
        platform: platformLabel(project.platform ?? ''),
      })
    : t('clientKeys.exampleJs');

  return (
    <div className="max-w-3xl">
      <SettingSection
        title={t('clientKeys.dsn')}
        description={t('clientKeys.dsnDescription')}
      >
        <div className="mt-3 flex items-center gap-2 rounded-lg border bg-muted p-3">
          <code className="flex-1 truncate font-mono text-xs">
            {project.dsn}
          </code>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => copy('dsn', project.dsn, t('clientKeys.dsn'))}
            className="shrink-0"
            aria-label={t('clientKeys.copyDsn')}
          >
            {isCopied('dsn') ? (
              <Check className="size-4 text-primary" />
            ) : (
              <Copy className="size-4" />
            )}
          </Button>
        </div>
      </SettingSection>

      {snippet?.install && (
        <SettingSection
          title={t('clientKeys.install')}
          description={t('clientKeys.installDescription')}
        >
          <div className="mt-3 flex items-center gap-2 rounded-lg border bg-muted p-3">
            <code className="flex-1 overflow-x-auto font-mono text-xs">
              {snippet.install}
            </code>
            <Button
              variant="ghost"
              size="sm"
              onClick={() =>
                copy(
                  'install',
                  snippet.install ?? '',
                  t('clientKeys.commandLabel'),
                )
              }
              className="shrink-0"
              aria-label={t('clientKeys.copyInstallCommand')}
            >
              {isCopied('install') ? (
                <Check className="size-4 text-primary" />
              ) : (
                <Copy className="size-4" />
              )}
            </Button>
          </div>
        </SettingSection>
      )}

      <SetupSection
        exampleTitle={exampleTitle}
        codeExample={codeExample}
        codeLanguage={codeLanguage}
        docsUrl={docsUrl}
        platform={project.platform}
        isDark={isDark}
        onCopy={() =>
          codeExample && copy('code', codeExample, t('clientKeys.exampleLabel'))
        }
        copied={isCopied('code')}
      />
    </div>
  );
}

interface SetupSectionProps {
  exampleTitle: string;
  /** Absent for a platform this build has no snippet for. */
  codeExample: string | undefined;
  codeLanguage: string;
  docsUrl: string | undefined;
  platform: string | null;
  isDark: boolean;
  onCopy: () => void;
  copied: boolean;
}

/**
 * The setup snippet, and the docs link that stands in for it.
 *
 * The link is the one part that always renders: a platform with no snippet
 * still has documentation, and it is the whole of what this section can offer
 * such a project.
 */
function SetupSection({
  exampleTitle,
  codeExample,
  codeLanguage,
  docsUrl,
  platform,
  isDark,
  onCopy,
  copied,
}: SetupSectionProps) {
  const t = useTranslations('projects');

  return (
    <SettingSection
      title={codeExample ? exampleTitle : t('clientKeys.setup')}
      description={
        codeExample ? t('clientKeys.dsnFilled') : t('clientKeys.pointSdk')
      }
    >
      <div className="mt-3">
        {codeExample && (
          <>
            <div className="mb-2 flex justify-end">
              <Button
                variant="ghost"
                size="sm"
                onClick={onCopy}
                className="h-6 text-xs"
              >
                {copied ? (
                  <>
                    <Check className="mr-1 size-3 text-primary" />
                    {t('clientKeys.copied')}
                  </>
                ) : (
                  <>
                    <Copy className="mr-1 size-3" />
                    {t('clientKeys.copy')}
                  </>
                )}
              </Button>
            </div>
            <div className="overflow-hidden overflow-x-auto rounded-lg border">
              <CodeHighlighter
                code={codeExample}
                language={codeLanguage}
                dark={isDark}
                customStyle={{
                  margin: 0,
                  padding: '1rem',
                  fontSize: '0.75rem',
                  background: isDark ? '#1e1e1e' : '#ffffff',
                }}
              />
            </div>
          </>
        )}

        <p
          className={`text-xs text-muted-foreground${codeExample ? ' mt-3' : ''}`}
        >
          {t('clientKeys.compatLead')}{' '}
          <a
            href={docsUrl ?? 'https://docs.sentry.io/platforms/'}
            target="_blank"
            rel="noopener noreferrer"
            className="text-primary hover:underline"
          >
            {docsUrl
              ? t('clientKeys.docsPlatform', {
                  platform: platformLabel(platform ?? ''),
                })
              : t('clientKeys.docsSentry')}
          </a>{' '}
          {codeExample
            ? t('clientKeys.compatFull')
            : t('clientKeys.compatSteps')}
        </p>
      </div>
    </SettingSection>
  );
}
