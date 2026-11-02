import { ChevronDownIcon, ChevronRightIcon, TrendUpIcon } from '@rustrak/ui';
import { useState } from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useTranslations } from 'use-intl';
import {
  buildFrameContextLines,
  type FrameContextLine,
  type StackFrame,
} from '@/features/event/lib/format-stack-trace';
import { cn } from '@/shared/lib/utils';
import { codeTheme } from '@/shared/ui/components/code-theme';

const LANGUAGE_BY_EXTENSION: Record<string, string> = {
  js: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  tsx: 'tsx',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  rs: 'rust',
  java: 'java',
  kt: 'kotlin',
  swift: 'swift',
  cs: 'csharp',
  cpp: 'cpp',
  c: 'c',
  h: 'c',
  hpp: 'cpp',
  php: 'php',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  sql: 'sql',
  json: 'json',
  yaml: 'yaml',
  yml: 'yaml',
  xml: 'xml',
  html: 'html',
  css: 'css',
  scss: 'scss',
  less: 'less',
  md: 'markdown',
};

/** The highlighter's language for a file, from its extension. */
function detectLanguage(filename?: string): string {
  const ext = filename?.split('.').pop()?.toLowerCase() ?? '';
  return LANGUAGE_BY_EXTENSION[ext] ?? 'text';
}

/** The frame's local variables, or `null` when it captured none. */
function frameVariables(frame: StackFrame): Record<string, unknown> | null {
  return frame.vars && Object.keys(frame.vars).length > 0 ? frame.vars : null;
}

/** Best-effort label for a frame's origin — `filename` first, falling back
 *  to `module`/`package` for platforms (JVM, .NET, native) that identify
 *  frames that way instead. */
function frameLocationLabel(frame: StackFrame): string {
  return frame.filename || frame.module || frame.package || '<unknown>';
}

/** Where a frame's line lives in the project's repository, if it has one. */
export type FrameSourceUrl = (frame: StackFrame) => string | null;

export function StackFrameItem({
  frame,
  index,
  sourceUrl,
}: {
  frame: StackFrame;
  index: number;
  sourceUrl?: FrameSourceUrl;
}) {
  const t = useTranslations('events');
  const [isExpanded, setIsExpanded] = useState(frame.in_app ?? false);
  const contextLines = buildFrameContextLines(frame);
  const vars = frameVariables(frame);
  const hasContext = contextLines.length > 0 || vars !== null;
  const Chevron = isExpanded ? ChevronDownIcon : ChevronRightIcon;
  // Library frames are not in the repository; only the app's own link out.
  const href = frame.in_app ? sourceUrl?.(frame) : null;

  return (
    // The app's own frames read at full weight; library frames recede, so
    // the eye lands on the code the reader can change.
    <div className={frame.in_app ? 'bg-surface' : 'bg-panel'}>
      {/* The link sits beside the toggle, not in it: an anchor inside a
          button is invalid and would toggle the frame on every click. */}
      <div className="flex items-center hover:bg-surface-hover">
        <button
          type="button"
          onClick={() => setIsExpanded((expanded) => !expanded)}
          className="flex min-w-0 flex-1 items-center gap-4 px-4 py-2.5 text-left"
        >
          <FrameSignature frame={frame} index={index} />
          {hasContext && (
            <span className="text-fg-ghost">
              <Chevron className="size-4" />
            </span>
          )}
        </button>
        {href && (
          <a
            href={href}
            target="_blank"
            rel="noreferrer"
            title={t('openInGitHub')}
            aria-label={t('openInGitHub')}
            className="px-3 py-2.5 text-fg-subtle hover:text-fg"
          >
            <TrendUpIcon className="size-4" />
          </a>
        )}
      </div>

      {isExpanded && contextLines.length > 0 && (
        <FrameSource
          lines={contextLines}
          language={detectLanguage(frame.filename)}
        />
      )}
      {isExpanded && vars && <FrameVariables vars={vars} />}
    </div>
  );
}

/** The frame's position, function and location, as the header reads them. */
function FrameSignature({
  frame,
  index,
}: {
  frame: StackFrame;
  index: number;
}) {
  return (
    <>
      <span className="w-6 shrink-0 font-mono text-mono-sm text-fg-ghost tabular-nums">
        {String(index).padStart(2, '0')}
      </span>

      <div className="min-w-0 flex-1">
        <p
          className={cn(
            'truncate font-mono text-mono',
            frame.in_app ? 'font-medium text-fg' : 'text-fg-subtle',
          )}
        >
          {frame.function || frame.raw_function || '<anonymous>'}
        </p>
        <p className="truncate font-mono text-mono-sm text-fg-meta">
          {frameLocationLabel(frame)}
          {frame.lineno != null && `:${frame.lineno}`}
          {frame.colno != null && `:${frame.colno}`}
        </p>
      </div>
    </>
  );
}

/** The source around the frame's line, syntax highlighted. */
function FrameSource({
  lines,
  language,
}: {
  lines: FrameContextLine[];
  language: string;
}) {
  return (
    // The code's own ground (`bg-code`): sunk into the frame card in dark,
    // paper inside the grey card in light. The highlighter's colours are
    // tokens too, so both follow the theme.
    <div className="overflow-x-auto border-border-divider border-t bg-code py-1.5 font-mono text-mono-sm leading-relaxed">
      {lines.map((line, i) => (
        // Deliberately not `line.lineNumber`. `buildFrameContextLines`
        // clamps its start at 0 when `pre_context` is longer than
        // `lineno`, so a malformed payload yields repeated line numbers
        // (0,1,2,3,4,2,3,4) and a stable-looking key would collide on
        // exactly the input the clamp exists to survive. The list is
        // built once per frame and never reorders.
        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
        <CodeLine
          key={i}
          lineNumber={line.lineNumber}
          code={line.code}
          language={language}
          isHighlighted={line.isHighlighted}
        />
      ))}
    </div>
  );
}

function FrameVariables({ vars }: { vars: Record<string, unknown> }) {
  const t = useTranslations('events');
  return (
    <div className="space-y-1.5 border-border-divider border-t px-4 py-3">
      <p className="text-column text-fg-meta">{t('stackTrace.variables')}</p>
      <dl className="space-y-1">
        {Object.entries(vars).map(([key, value]) => (
          <div key={key} className="flex gap-2 font-mono text-mono-sm">
            <dt className="shrink-0 text-fg-meta">{key}</dt>
            <dd className="min-w-0 break-all text-fg">
              {typeof value === 'string' ? value : JSON.stringify(value)}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function CodeLine({
  lineNumber,
  code,
  language,
  isHighlighted,
}: {
  lineNumber: number;
  code: string;
  language: string;
  isHighlighted: boolean;
}) {
  return (
    <div
      className={cn(
        'relative flex',
        // The line that threw, marked as the error it is.
        isHighlighted ? 'bg-sev-error-surface' : 'opacity-70',
      )}
    >
      {/* Highlight indicator */}
      {isHighlighted && (
        <div className="absolute inset-y-0 left-0 w-0.5 bg-sev-error" />
      )}

      {/* Line number */}
      <span
        className={cn(
          'w-12 shrink-0 text-right pr-4 pl-3 select-none py-0.5',
          isHighlighted ? 'font-medium text-sev-error-fg' : 'text-fg-ghost',
        )}
      >
        {lineNumber}
      </span>

      {/* Code with syntax highlighting */}
      <div className="flex-1 py-0.5 pr-4 overflow-x-auto">
        <SyntaxHighlighter
          language={language}
          style={codeTheme}
          customStyle={{
            margin: 0,
            padding: 0,
            background: 'transparent',
            fontSize: 'inherit',
            lineHeight: 'inherit',
          }}
          codeTagProps={{
            style: {
              fontFamily: 'inherit',
              whiteSpace: 'pre',
            },
          }}
        >
          {code || ' '}
        </SyntaxHighlighter>
      </div>
    </div>
  );
}
