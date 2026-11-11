export interface StackFrame {
  filename?: string;
  function?: string;
  module?: string;
  package?: string;
  raw_function?: string;
  lineno?: number;
  colno?: number;
  in_app?: boolean;
  context_line?: string;
  pre_context?: string[];
  post_context?: string[];
  vars?: Record<string, unknown>;
}

export interface ExceptionValue {
  type?: string;
  value?: string;
  /** Cross-references a `Thread.id` — links this exception to the thread that raised it. */
  thread_id?: string | number;
  stacktrace?: {
    frames?: StackFrame[];
  };
}

export interface ExceptionChain {
  values?: ExceptionValue[];
}

export interface Thread {
  id?: string | number;
  name?: string;
  crashed?: boolean;
  current?: boolean;
  main?: boolean;
  state?: string;
  stacktrace?: {
    frames?: StackFrame[];
  };
}

/**
 * A single line of rendered source code context for a frame, with its real
 * (not guessed) line number.
 */
export interface FrameContextLine {
  lineNumber: number;
  code: string;
  isHighlighted: boolean;
}

/**
 * Platforms where Sentry's own UI shows frames oldest-first by default
 * (matches how the platform's native traceback reads). Every other
 * platform defaults to newest-first.
 *
 * Mirrors `is_newest_frame_first()` in the Sentry monolith
 * (src/sentry/interfaces/stacktrace.py) — minus the per-user
 * `stacktrace_order` preference override, which has no equivalent in
 * Rustrak today.
 */
const OLDEST_FRAME_FIRST_PLATFORMS = new Set(['python']);

function shouldShowNewestFirst(platform?: string): boolean {
  return !OLDEST_FRAME_FIRST_PLATFORMS.has(platform ?? '');
}

/**
 * Order frames for display, respecting platform convention instead of
 * always reversing. Protocol order is always oldest-to-newest
 * (last frame = crash site); this only decides what to show first.
 */
export function orderFramesForDisplay(
  frames: StackFrame[],
  platform?: string,
): StackFrame[] {
  return shouldShowNewestFirst(platform) ? [...frames].reverse() : frames;
}

/**
 * Build the renderable context-line window for a frame, with real line
 * numbers. Mirrors `get_context()` in the Sentry monolith
 * (src/sentry/interfaces/stacktrace.py): if there's no `lineno`, there is
 * no context to show — guessing a base line (e.g. defaulting to 0) produces
 * fabricated line numbers next to real code, which is worse than showing
 * nothing.
 */
export function buildFrameContextLines(frame: StackFrame): FrameContextLine[] {
  if (frame.lineno === undefined) return [];
  if (
    frame.context_line === undefined &&
    !frame.pre_context?.length &&
    !frame.post_context?.length
  ) {
    return [];
  }

  const lines: FrameContextLine[] = [];
  const preContext = frame.pre_context ?? [];
  // Clamp instead of going negative — mirrors Sentry's get_context(), which
  // guards against pre_context arrays longer than the real lineno.
  let atLineNumber = Math.max(0, frame.lineno - preContext.length);

  for (const code of preContext) {
    lines.push({ lineNumber: atLineNumber, code, isHighlighted: false });
    atLineNumber++;
  }

  if (frame.context_line !== undefined) {
    lines.push({
      lineNumber: frame.lineno,
      code: frame.context_line,
      isHighlighted: true,
    });
  }
  atLineNumber = frame.lineno + 1;

  for (const code of frame.post_context ?? []) {
    lines.push({ lineNumber: atLineNumber, code, isHighlighted: false });
    atLineNumber++;
  }

  return lines;
}

/**
 * Pick which thread to show by default when an event reports its crash via
 * `threads` instead of (or in addition to) `exception`.
 *
 * Mirrors `findBestThread()` in Sentry's frontend
 * (static/app/components/events/interfaces/threads/threadSelector/findBestThread.tsx):
 * the crashed thread wins, then any thread that at least has a stacktrace,
 * then just the first thread.
 */
export function findBestThread(threads: Thread[]): Thread | undefined {
  return (
    threads.find((t) => t.crashed) ??
    threads.find((t) => t.stacktrace) ??
    threads[0]
  );
}

/**
 * Find the exception value that belongs to a given thread, so a crashed
 * thread can show the exception's type/value/frames instead of (or in
 * place of) its own raw stacktrace.
 *
 * Mirrors the matching rules in Sentry's `getThreadException()`
 * (static/app/components/events/interfaces/threads/threadSelector/getThreadException.tsx):
 * prefer an explicit `thread_id` match; otherwise, if there's exactly one
 * exception value with no `thread_id` at all and this thread crashed,
 * attribute it to that thread.
 */
export function matchExceptionForThread(
  exception: ExceptionChain | undefined,
  thread: Thread | undefined,
): ExceptionValue | undefined {
  const values = exception?.values;
  if (!values?.length || !thread) return undefined;

  // Coerce both sides before comparing. Relay's own `ThreadId` protocol type
  // (relay-event-schema/src/protocol/thread.rs) explicitly allows either a
  // string or an integer on the wire, and — unlike Sentry's real frontend
  // (static/app/types/event.tsx, which just declares `Thread.id: number` /
  // `threadId: number | null` without runtime enforcement) — we don't have
  // an upstream normalization step that guarantees both sides end up the
  // same type before this comparison runs. A strict `===` here would silently
  // fail to link the exception whenever an SDK's `id` and `thread_id` happen
  // to differ in representation, even though they refer to the same thread.
  const matched = values.find(
    (v) =>
      v.thread_id !== undefined && String(v.thread_id) === String(thread.id),
  );
  if (matched) return matched;

  if (
    values.length === 1 &&
    values[0].thread_id === undefined &&
    thread.crashed
  ) {
    return values[0];
  }

  return undefined;
}

/**
 * Plain-text rendering of the full exception chain, frame order matching
 * what the UI shows for this platform.
 */
export function formatStackTraceAsText(
  exception?: ExceptionChain,
  platform?: string,
): string {
  const exceptions = exception?.values ?? [];
  return exceptions
    .map((exc) => {
      const header = [exc.type, exc.value].filter(Boolean).join(': ');
      const frames = orderFramesForDisplay(
        exc.stacktrace?.frames ?? [],
        platform,
      );
      const frameLines = frames.map((frame) => {
        const location = [frame.filename, frame.lineno, frame.colno]
          .filter((part) => part !== undefined && part !== '')
          .join(':');
        return `  at ${frame.function || '<anonymous>'} (${location})`;
      });
      return [header, ...frameLines].join('\n');
    })
    .join('\n\n');
}
