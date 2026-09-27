import { z } from 'zod';
import { parseEpochSeconds } from '@/shared/lib/protocol-timestamp';

/**
 * Schema for a stack frame in an exception.
 */
const stackFrameSchema = z.object({
  filename: z.string().optional(),
  function: z.string().optional(),
  module: z.string().optional(),
  package: z.string().optional(),
  raw_function: z.string().optional(),
  lineno: z.number().optional(),
  colno: z.number().optional(),
  in_app: z.boolean().optional(),
  context_line: z.string().optional(),
  pre_context: z.array(z.string()).optional(),
  post_context: z.array(z.string()).optional(),
  vars: z.record(z.string(), z.unknown()).optional(),
});

/**
 * Schema for an exception value.
 */
const exceptionValueSchema = z.object({
  type: z.string().optional(),
  value: z.string().optional(),
  /** Cross-references a `Thread.id` — links this exception to the thread that raised it. */
  thread_id: z.union([z.string(), z.number()]).optional(),
  stacktrace: z
    .object({
      frames: z.array(stackFrameSchema).optional(),
    })
    .optional(),
});

/**
 * Schema for the exception object in a Sentry event.
 */
const exceptionSchema = z
  .object({
    values: z.array(exceptionValueSchema).optional(),
  })
  .optional();

/**
 * Schema for a single thread entry (native crashes, Go panics, JVM thread
 * dumps — stack traces that arrive outside the `exception` object).
 */
const threadSchema = z.object({
  id: z.union([z.string(), z.number()]).optional(),
  name: z.string().optional(),
  crashed: z.boolean().optional(),
  current: z.boolean().optional(),
  main: z.boolean().optional(),
  state: z.string().optional(),
  stacktrace: z
    .object({
      frames: z.array(stackFrameSchema).optional(),
    })
    .optional(),
});

/**
 * Schema for `threads` (can be array or object with values, same shape as breadcrumbs).
 */
const threadsSchema = z.union([
  z.array(threadSchema),
  z.object({ values: z.array(threadSchema).optional() }),
]);

/**
 * Schema for a breadcrumb entry.
 *
 * `timestamp` is deliberately left open here and read by `parseEpochSeconds`
 * in {@link normalizeBreadcrumbs}. The protocol allows epoch seconds or an
 * RFC 3339 string, and which one arrives depends on the SDK (`sentry-python`
 * sends the string). A `z.number()` here made one string crumb fail the whole
 * `breadcrumbs` parse, and the section vanished from the page for every event
 * those SDKs sent. Relay reads a timestamp it cannot parse as absent and keeps
 * the crumb, so an unreadable value is `undefined` rather than a reason to
 * drop anything.
 */
const breadcrumbSchema = z.object({
  timestamp: z.unknown().optional(),
  type: z.string().optional(),
  category: z.string().optional(),
  message: z.string().optional(),
  level: z.string().optional(),
  data: z.record(z.string(), z.unknown()).optional(),
});

/**
 * Schema for breadcrumbs (can be array or object with values).
 */
const breadcrumbsSchema = z.union([
  z.array(breadcrumbSchema),
  z.object({ values: z.array(breadcrumbSchema).optional() }),
]);

/**
 * Schema for user information.
 */
const userSchema = z
  .object({
    id: z.string().optional(),
    email: z.string().optional(),
    ip_address: z.string().optional(),
  })
  .optional();

/**
 * Schema for tags. Sentry SDKs can send tag values as string, boolean, or number.
 */
const tagsSchema = z
  .record(z.string(), z.union([z.string(), z.boolean(), z.number()]))
  .optional();

/**
 * Schema for contexts.
 */
const contextsSchema = z
  .record(z.string(), z.record(z.string(), z.unknown()))
  .optional();

/**
 * Schema for `modules` — a flat package-name → version map SDKs attach to
 * report the dependency versions running when the event was captured.
 */
const modulesSchema = z.record(z.string(), z.string()).optional();

/**
 * Parsed and validated event data types.
 */
type ValidatedEventBreadcrumbs = z.infer<typeof breadcrumbsSchema>;
type ValidatedEventThreads = z.infer<typeof threadsSchema>;

/**
 * Parse and validate event data from the Sentry event JSON.
 * Returns validated and type-safe data structures.
 */
export function parseEventData(eventData: Record<string, unknown>) {
  const exception = exceptionSchema.safeParse(eventData.exception);
  const breadcrumbs = breadcrumbsSchema.safeParse(eventData.breadcrumbs);
  const threads = threadsSchema.safeParse(eventData.threads);
  const contexts = contextsSchema.safeParse(eventData.contexts);
  const modules = modulesSchema.safeParse(eventData.modules);
  const tagsResult = tagsSchema.safeParse(eventData.tags);
  const user = userSchema.safeParse(eventData.user);

  const tags =
    tagsResult.success && tagsResult.data
      ? (Object.fromEntries(
          Object.entries(tagsResult.data).map(([k, v]) => [k, String(v)]),
        ) as Record<string, string>)
      : undefined;

  return {
    exception: exception.success ? exception.data : undefined,
    breadcrumbs: breadcrumbs.success ? breadcrumbs.data : undefined,
    threads: threads.success ? threads.data : undefined,
    contexts: contexts.success ? contexts.data : undefined,
    modules: modules.success ? modules.data : undefined,
    tags,
    user: user.success ? user.data : undefined,
  };
}

/**
 * Normalize breadcrumbs to always be an array, with every timestamp read
 * into epoch seconds the way Relay would have re-emitted it.
 */
export function normalizeBreadcrumbs(
  breadcrumbs: ValidatedEventBreadcrumbs | undefined,
): Array<{
  timestamp?: number;
  type?: string;
  category?: string;
  message?: string;
  level?: string;
  data?: Record<string, unknown>;
}> {
  if (!breadcrumbs) return [];
  const items = Array.isArray(breadcrumbs)
    ? breadcrumbs
    : (breadcrumbs.values ?? []);
  return items.map((crumb) => ({
    ...crumb,
    timestamp: parseEpochSeconds(crumb.timestamp),
  }));
}

/**
 * Normalize threads to always be an array.
 */
export function normalizeThreads(
  threads: ValidatedEventThreads | undefined,
): z.infer<typeof threadSchema>[] {
  if (!threads) return [];
  if (Array.isArray(threads)) return threads;
  return threads.values ?? [];
}
