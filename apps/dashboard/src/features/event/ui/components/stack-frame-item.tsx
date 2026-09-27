import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  buildFrameContextLines,
  type FrameContextLine,
  type StackFrame,
} from '@/features/event/lib/format-stack-trace';
import { cn } from '@/shared/lib/utils';
import { CodeHighlighter } from '@/shared/ui/components/code-highlighter';

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

export function StackFrameItem({
  frame,
  index,
}: {
  frame: StackFrame;
  index: number;
}) {
  const [isExpanded, setIsExpanded] = useState(frame.in_app ?? false);
  const contextLines = buildFrameContextLines(frame);
  const vars = frameVariables(frame);
  const hasContext = contextLines.length > 0 || vars !== null;
  const Chevron = isExpanded ? ChevronDown : ChevronRight;

  return (
    <div
      className={cn(
        'border rounded-lg overflow-hidden transition-colors',
        frame.in_app
          ? 'border-primary/40 bg-primary/5'
          : 'border-border bg-card/50 opacity-60',
      )}
    >
      <button
        type="button"
        onClick={() => setIsExpanded((expanded) => !expanded)}
        className="w-full px-4 py-3 flex items-center gap-4 text-left hover:bg-muted/30 transition-colors"
      >
        <FrameSignature frame={frame} index={index} />
        {hasContext && (
          <span className="text-muted-foreground">
            <Chevron className="size-4" />
          </span>
        )}
      </button>

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
      <span
        className={cn(
          'text-xs font-mono',
          frame.in_app ? 'text-primary' : 'text-muted-foreground',
        )}
      >
        {String(index).padStart(2, '0')}
      </span>

      <div className="flex-1 min-w-0">
        <p className="font-mono text-sm font-semibold truncate">
          {frame.function || frame.raw_function || '<anonymous>'}
        </p>
        <p className="text-xs text-muted-foreground font-mono truncate">
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
    <div className="bg-zinc-900 font-mono text-xs leading-relaxed overflow-x-auto">
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
    <div className="border-t px-4 py-3 space-y-1.5">
      <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
        {t('stackTrace.variables')}
      </p>
      <dl className="space-y-1">
        {Object.entries(vars).map(([key, value]) => (
          <div key={key} className="flex gap-2 text-xs font-mono">
            <dt className="shrink-0 text-muted-foreground">{key}</dt>
            <dd className="min-w-0 break-all text-foreground">
              {typeof value === 'string' ? value : JSON.stringify(value)}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

const CODE_STYLE = {
  margin: 0,
  padding: 0,
  background: 'transparent',
  fontSize: 'inherit',
  lineHeight: 'inherit',
};

const CODE_TAG_PROPS = {
  style: { fontFamily: 'inherit', whiteSpace: 'pre' as const },
};

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
        'flex relative',
        isHighlighted ? 'bg-primary/15' : 'opacity-60',
      )}
    >
      {/* Highlight indicator */}
      {isHighlighted && (
        <div className="absolute left-0 top-0 bottom-0 w-[3px] bg-primary" />
      )}

      {/* Line number */}
      <span
        className={cn(
          'w-12 shrink-0 text-right pr-4 pl-3 select-none py-0.5',
          isHighlighted ? 'text-primary font-medium' : 'text-muted-foreground',
        )}
      >
        {lineNumber}
      </span>

      {/* Code with syntax highlighting */}
      <div className="flex-1 py-0.5 pr-4 overflow-x-auto">
        <CodeHighlighter
          code={code || ' '}
          language={language}
          dark
          customStyle={CODE_STYLE}
          codeTagProps={CODE_TAG_PROPS}
        />
      </div>
    </div>
  );
}
