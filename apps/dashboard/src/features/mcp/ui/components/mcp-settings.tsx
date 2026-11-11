import type { McpStatus } from '@rustrak/client';
import {
  AgentsIcon,
  Button,
  CopyIcon,
  Dialog,
  DialogBody,
  DialogHeader,
  HelpIcon,
  SegmentedControl,
  SegmentedItem,
  SelectedIcon,
  Switch,
  Tag,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useState } from 'react';
import { useTranslations } from 'use-intl';
import { mcpMutations } from '@/features/mcp/api/mutations';
import {
  type ConnectMethod,
  mcpSnippet,
  type SnippetClient,
} from '@/features/mcp/lib/snippets';
import { CodeBlock } from '@/shared/ui/components/code-block';
import { Link } from '@/shared/ui/components/link';
import {
  SettingRow,
  SettingSection,
  SettingsEmpty,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { type CopyFlags, useCopyFlags } from '@/shared/ui/hooks/use-copy-flags';
import { useSyntaxHighlighter } from '@/shared/ui/hooks/use-syntax-highlighter';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

interface McpSettingsProps {
  settings: McpStatus;
  isAdmin: boolean;
  /** The reader's connected assistants, shown while the server is on. */
  connected: ReactNode;
}

type Client = SnippetClient | 'web';

/** The clients, in the order people reach for them. The web apps need no snippet. */
const CLIENTS: readonly Client[] = ['web', 'claudeCode', 'json', 'vscode'];

/**
 * The MCP page, whole: an on/off in the header, and -- while on -- the one
 * URL to give an assistant, what to paste into the client you use, and who is
 * connected.
 *
 * Signing in is the way in whenever the server can offer it, so it is the
 * only one shown; the API token appears only on a server without
 * `PUBLIC_URL`, where signing in cannot work.
 */
export function McpSettings({
  settings,
  isAdmin,
  connected,
}: McpSettingsProps) {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const update = useMutation(mcpMutations.updateSettings());
  // The switch moves at once; the server's answer settles it either way.
  const enabled = update.isPending ? update.variables : settings.enabled;
  const [helpOpen, setHelpOpen] = useState(false);
  const copyFlags = useCopyFlags();
  const signInAvailable = settings.url !== null;
  const url = settings.url ?? `${window.location.origin}/mcp`;

  const toggle = (next: boolean) =>
    update.mutate(next, {
      onError: (error) => toastFailure(t('mcp.server.updateFailed'), error),
    });

  return (
    <>
      <SettingsHeader
        title={t('mcp.title')}
        description={t('mcp.subtitle')}
        actions={
          <>
            <Button
              variant="ghost"
              icon={HelpIcon}
              aria-label={t('mcp.page.help')}
              onClick={() => setHelpOpen(true)}
            />
            {isAdmin ? (
              <Switch
                checked={enabled}
                onCheckedChange={toggle}
                disabled={update.isPending}
                aria-label={t('mcp.server.toggleLabel')}
              />
            ) : null}
          </>
        }
      />

      {enabled ? (
        <SettingSection title={t('mcp.page.connect')}>
          <SettingRow title={t('mcp.server.endpoint')}>
            <div className="flex items-center gap-2 rounded-lg border border-border-subtle bg-surface-raised py-1 ps-3 pe-1">
              <code className="min-w-0 flex-1 truncate font-mono text-mono text-fg">
                {url}
              </code>
              <CopyButton
                flags={copyFlags}
                target="url"
                value={url}
                label={t('mcp.server.endpoint')}
              />
            </div>
          </SettingRow>
          {/* The main way in only; the other one, and why there are two,
                live behind the help button. */}
          <ClientGuide
            method={signInAvailable ? 'oauth' : 'token'}
            url={url}
            copyFlags={copyFlags}
          />
        </SettingSection>
      ) : (
        <SettingSection title={t('mcp.page.connect')}>
          <SettingsEmpty
            icon={<AgentsIcon size="2xl" />}
            title={t('mcp.page.offTitle')}
            description={
              isAdmin ? t('mcp.page.offAdmin') : t('mcp.page.offMember')
            }
            action={
              isAdmin ? (
                <Button
                  variant="primary"
                  loading={update.isPending}
                  onClick={() => toggle(true)}
                >
                  {t('mcp.page.enable')}
                </Button>
              ) : null
            }
          />
        </SettingSection>
      )}

      {/* With MCP off too: the grants are still stored, and withdrawing one
          must not wait for an admin to turn the server back on. */}
      {connected}

      <Dialog open={helpOpen} onOpenChange={setHelpOpen} size="lg">
        <DialogHeader
          title={t('mcp.page.helpTitle')}
          description={t('mcp.page.helpIntro')}
        />
        <DialogBody className="flex flex-col gap-5">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <WayIn
              name={t('mcp.page.oauthName')}
              badge={signInAvailable ? t('mcp.oauth.recommended') : undefined}
              points={[
                t('mcp.page.oauth1'),
                t('mcp.page.oauth2'),
                t('mcp.page.oauth3'),
              ]}
              note={signInAvailable ? undefined : t('mcp.page.publicUrl')}
            />
            <WayIn
              name={t('mcp.page.tokenName')}
              points={[
                t('mcp.page.token1'),
                t('mcp.page.token2'),
                t('mcp.page.token3'),
              ]}
            />
          </div>
          {signInAvailable ? (
            <div className="flex flex-col gap-1">
              <p className="text-label text-fg-muted">
                {t('mcp.page.tokenSetup')}
              </p>
              <div className="-mx-5">
                <ClientGuide method="token" url={url} copyFlags={copyFlags} />
              </div>
            </div>
          ) : null}
        </DialogBody>
      </Dialog>
    </>
  );
}

/** One way to connect, as three short facts. */
function WayIn({
  name,
  badge,
  points,
  note,
}: {
  name: string;
  badge?: string;
  points: string[];
  /** Why it is not available here, when it is not. */
  note?: string;
}) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-border-subtle p-4">
      <div className="flex items-center gap-2">
        <span className="text-card-title text-fg">{name}</span>
        {badge ? (
          <Tag variant="soft" tone="brand">
            {badge}
          </Tag>
        ) : null}
      </div>
      <ul className="flex flex-col gap-1.5">
        {points.map((point) => (
          <li key={point} className="flex gap-2 text-meta text-fg-subtle">
            <SelectedIcon size="sm" className="mt-0.5 shrink-0 text-fg-meta" />
            {point}
          </li>
        ))}
      </ul>
      {note ? <p className="text-hint text-sev-warning-fg">{note}</p> : null}
    </div>
  );
}

/**
 * One way in, for the client you use: pick it, see the one thing to do in it.
 * The web apps only take a URL, so they appear under signing in alone.
 */
function ClientGuide({
  method,
  url,
  copyFlags,
}: {
  method: ConnectMethod;
  url: string;
  copyFlags: CopyFlags;
}) {
  const t = useTranslations('settings');
  const highlighter = useSyntaxHighlighter();
  const clients =
    method === 'oauth' ? CLIENTS : CLIENTS.filter((c) => c !== 'web');
  const [client, setClient] = useState<Client>(clients[0] ?? 'claudeCode');
  const snippet = client === 'web' ? null : mcpSnippet(method, client, url);

  return (
    <div className="flex flex-col gap-3 px-5 py-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SegmentedControl
          aria-label={
            method === 'oauth' ? t('mcp.oauth.title') : t('mcp.token.title')
          }
          value={client}
          onValueChange={(value) => setClient(value as Client)}
        >
          {clients.map((id) => (
            <SegmentedItem key={id} value={id}>
              {t(`mcp.clients.${id}`)}
            </SegmentedItem>
          ))}
        </SegmentedControl>
        {snippet ? (
          <CopyButton
            flags={copyFlags}
            target={`${method}-${client}`}
            value={snippet.code}
            label={t(`mcp.clients.${client}`)}
            withText
          />
        ) : null}
      </div>

      {snippet ? (
        <div className="overflow-x-auto rounded-xl border border-border-subtle">
          <CodeBlock
            code={snippet.code}
            language={snippet.language}
            highlighter={highlighter}
          />
        </div>
      ) : (
        <p className="text-meta text-fg-subtle">{t('mcp.page.webHint')}</p>
      )}

      {method === 'token' ? (
        <p className="text-meta text-fg-subtle">
          {t('mcp.token.step3')}{' '}
          <Link
            href="/settings/tokens"
            className="text-fg-brand hover:underline"
          >
            {t('mcp.token.createToken')}
          </Link>
        </p>
      ) : null}
    </div>
  );
}

function CopyButton({
  flags,
  target,
  value,
  label,
  withText = false,
}: {
  flags: CopyFlags;
  target: string;
  value: string;
  label: string;
  withText?: boolean;
}) {
  const t = useTranslations('settings');
  const copied = flags.isCopied(target);

  return (
    <Button
      variant="ghost"
      size="sm"
      icon={copied ? SelectedIcon : CopyIcon}
      onClick={() => flags.copy(target, value, label)}
      aria-label={t('mcp.copyLabel', { label })}
    >
      {withText ? (copied ? t('mcp.copied') : t('mcp.copy')) : undefined}
    </Button>
  );
}
