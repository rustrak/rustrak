import type { ProjectResponse } from '@rustrak/client';
import { Button, CopyIcon, SelectedIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { resolveSetupSnippet } from '@/features/project/lib/setup-snippet';
import { platformLabel } from '@/shared/config/platforms';
import { CodeBlock } from '@/shared/ui/components/code-block';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { useCopyFlags } from '@/shared/ui/hooks/use-copy-flags';
import {
  type SyntaxHighlighter,
  useSyntaxHighlighter,
} from '@/shared/ui/hooks/use-syntax-highlighter';

interface ClientKeysSettingsProps {
  project: ProjectResponse;
  dsn: string;
}

export function ClientKeysSettings({ project, dsn }: ClientKeysSettingsProps) {
  const t = useTranslations('projects');

  const highlighter = useSyntaxHighlighter();
  const { isCopied, copy } = useCopyFlags();

  const { snippet, docsUrl, codeExample, codeLanguage } = resolveSetupSnippet(
    project.platform,
    dsn,
  );

  const exampleTitle = snippet
    ? t('clientKeys.examplePlatform', {
        platform: platformLabel(project.platform ?? ''),
      })
    : t('clientKeys.exampleJs');

  return (
    <>
      <SettingSection
        title={t('clientKeys.dsn')}
        description={t('clientKeys.dsnDescription')}
      >
        <CopyValue
          value={dsn}
          copied={isCopied('dsn')}
          ariaLabel={t('clientKeys.copyDsn')}
          onCopy={() => copy('dsn', dsn, t('clientKeys.dsn'))}
        />
      </SettingSection>

      {snippet?.install && (
        <SettingSection
          title={t('clientKeys.install')}
          description={t('clientKeys.installDescription')}
        >
          <CopyValue
            value={snippet.install}
            copied={isCopied('install')}
            ariaLabel={t('clientKeys.copyInstallCommand')}
            onCopy={() =>
              copy(
                'install',
                snippet.install ?? '',
                t('clientKeys.commandLabel'),
              )
            }
          />
        </SettingSection>
      )}

      <SetupSection
        exampleTitle={exampleTitle}
        codeExample={codeExample}
        codeLanguage={codeLanguage}
        docsUrl={docsUrl}
        platform={project.platform}
        highlighter={highlighter}
        onCopy={() =>
          codeExample && copy('code', codeExample, t('clientKeys.exampleLabel'))
        }
        copied={isCopied('code')}
      />
    </>
  );
}

/** One value to paste somewhere else, in mono, with its copy button. */
function CopyValue({
  value,
  copied,
  ariaLabel,
  onCopy,
}: {
  value: string;
  copied: boolean;
  ariaLabel: string;
  onCopy: () => void;
}) {
  return (
    <div className="flex items-center gap-3 px-5 py-3">
      <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-mono text-fg">
        {value}
      </code>
      <Button
        variant="ghost"
        size="sm"
        icon={copied ? SelectedIcon : CopyIcon}
        onClick={onCopy}
        aria-label={ariaLabel}
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
  highlighter: SyntaxHighlighter;
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
  highlighter,
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
      actions={
        codeExample ? (
          <Button
            variant="ghost"
            size="sm"
            icon={copied ? SelectedIcon : CopyIcon}
            onClick={onCopy}
          >
            {copied ? t('clientKeys.copied') : t('clientKeys.copy')}
          </Button>
        ) : null
      }
    >
      {codeExample && (
        <div className="overflow-x-auto border-b border-border-divider">
          <CodeBlock
            code={codeExample}
            language={codeLanguage}
            highlighter={highlighter}
          />
        </div>
      )}

      <p className="px-5 py-3 text-hint text-fg-subtle">
        {t('clientKeys.compatLead')}{' '}
        <a
          href={docsUrl ?? 'https://docs.sentry.io/platforms/'}
          target="_blank"
          rel="noopener noreferrer"
          className="text-fg-brand hover:underline"
        >
          {docsUrl
            ? t('clientKeys.docsPlatform', {
                platform: platformLabel(platform ?? ''),
              })
            : t('clientKeys.docsSentry')}
        </a>{' '}
        {codeExample ? t('clientKeys.compatFull') : t('clientKeys.compatSteps')}
      </p>
    </SettingSection>
  );
}
