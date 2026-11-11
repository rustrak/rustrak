import type { AlertIntegration, ProviderType } from '@rustrak/client';
import type { Translate } from '@/shared/lib/error-copy';

/**
 * Per-channel `routing_override`, keyed by integration id.
 *
 * Deliberately outside the form schema. What a channel needs to be routed
 * depends on the provider it points at, so it cannot be expressed as a fixed
 * set of Zod fields; it is assembled at submit time by `buildChannelsPayload`.
 */
export type RoutingMap = Record<number, Record<string, string>>;

/** Which routing inputs a channel shows, and which of them it insists on. */
export interface RoutingNeeds {
  needsChannel: boolean;
  needsRecipients: boolean;
  needsUrl: boolean;
  /** Whether the channel shows a routing section at all. */
  hasRoutingFields: boolean;
}

export function getSlackMethod(integration: AlertIntegration): string {
  return (
    ((integration.credentials as Record<string, unknown>).method as string) ??
    'webhook'
  );
}

/**
 * Everything one provider knows about routing, in one place. Keyed by
 * `ProviderType`, so a provider added to the client is a type error here
 * until it answers all three questions.
 */
interface RoutingBehaviour {
  /** Which override inputs to render for this integration. */
  needs: (integration: AlertIntegration) => RoutingNeeds;
  /** Why this routing cannot be saved, as a message, or `null`. */
  validate: (
    integration: AlertIntegration,
    routing: Record<string, string>,
    t: Translate,
  ) => string | null;
  /** What the API should store, with anything blank left out. */
  override: (
    integration: AlertIntegration,
    routing: Record<string, string>,
  ) => Record<string, unknown>;
  /** The inverse of `override`: a stored override, as the inputs hold it. */
  read: (override: Record<string, unknown>) => Record<string, string>;
}

/**
 * The default read: every override value an input could actually hold.
 *
 * Anything else stored under that channel is a field this build does not
 * render, and putting a number or an object into a text input would only lose
 * it on the next save.
 */
const stringFields = (
  override: Record<string, unknown>,
): Record<string, string> => {
  const fields: Record<string, string> = {};
  for (const [key, value] of Object.entries(override)) {
    if (typeof value === 'string') fields[key] = value;
  }
  return fields;
};

const isBlank = (value: string | undefined): boolean =>
  value == null || value.trim() === '';

/**
 * A URL a dispatcher could actually POST to.
 *
 * Parsed rather than prefix-matched: `https://` on its own, and anything with
 * a space in the authority, start with the right characters and are still not
 * addresses. A prefix test passes them through to the save, and the failure
 * surfaces later as a delivery that never arrives.
 */
const isHttpUrl = (url: string): boolean => {
  let protocol: string;
  try {
    protocol = new URL(url).protocol;
  } catch {
    return false;
  }
  return protocol === 'http:' || protocol === 'https:';
};

const storedUrl = (integration: AlertIntegration): string | undefined =>
  (integration.credentials as Record<string, unknown>).url as
    | string
    | undefined;

/** The comma-separated line the input holds, as the list the API wants. */
const recipientList = (line: string): string[] =>
  line
    .split(',')
    .map((address) => address.trim())
    .filter(Boolean);

/**
 * An incoming webhook already names its channel in the URL; only a bot token
 * can be pointed anywhere, so only a bot token has anything to route.
 */
const slackRouting: RoutingBehaviour = {
  needs: (integration) => {
    const isBot = getSlackMethod(integration) === 'bot_token';
    return {
      needsChannel: isBot,
      needsRecipients: false,
      needsUrl: false,
      hasRoutingFields: isBot,
    };
  },

  validate: (integration, routing, t) => {
    if (getSlackMethod(integration) !== 'bot_token') return null;
    if (isBlank(routing.channel)) {
      return t('routing.slackChannelRequired', { name: integration.name });
    }
    return null;
  },

  override: (integration, routing) => {
    if (getSlackMethod(integration) !== 'bot_token') return {};
    if (!routing.channel) return {};
    return { channel: routing.channel.trim() };
  },

  read: stringFields,
};

/** An SMTP account is a sender, never a destination: the rule supplies those. */
const emailRouting: RoutingBehaviour = {
  needs: () => ({
    needsChannel: false,
    needsRecipients: true,
    needsUrl: false,
    hasRoutingFields: true,
  }),

  validate: (integration, routing, t) => {
    const name = integration.name;
    if (isBlank(routing.recipients)) {
      return t('routing.recipientsRequired', { name });
    }

    // A line of commas parses to nothing, and an entry with no `@` is a typo
    // the server would only discover at send time.
    const addresses = recipientList(routing.recipients);
    if (addresses.length === 0 || addresses.some((a) => !a.includes('@'))) {
      return t('routing.invalidRecipients', { name });
    }
    return null;
  },

  override: (_integration, routing) =>
    routing.recipients ? { recipients: recipientList(routing.recipients) } : {},

  // The one provider whose override is not a string on the wire: a list there,
  // a comma-joined line in the input.
  read: (override): Record<string, string> => {
    const recipients = override.recipients;
    if (!Array.isArray(recipients)) return {};
    return { recipients: recipients.join(', ') };
  },
};

/**
 * A webhook that already carries a URL in its credentials still offers the
 * override field, but does not require it; one that does not, requires it.
 *
 * Custom webhook shares this behaviour outright: its credentials carry the
 * same `url`, and its template is not a routing field — a rule reroutes
 * where a payload goes, never what the integration renders.
 */
const webhookRouting: RoutingBehaviour = {
  needs: (integration) => ({
    needsChannel: false,
    needsRecipients: false,
    needsUrl: !storedUrl(integration),
    hasRoutingFields: true,
  }),

  validate: (integration, routing, t) => {
    const name = integration.name;
    const routeUrl = routing.url?.trim();

    if (!storedUrl(integration) && !routeUrl) {
      return t('routing.webhookUrlRequired', { name });
    }
    if (routeUrl && !isHttpUrl(routeUrl)) {
      return t('routing.invalidOverrideUrl', { name });
    }
    return null;
  },

  override: (_integration, routing) =>
    routing.url ? { url: routing.url.trim() } : {},

  read: stringFields,
};

const ROUTING_BEHAVIOUR: Record<ProviderType, RoutingBehaviour> = {
  slack: slackRouting,
  email: emailRouting,
  webhook: webhookRouting,
  custom_webhook: webhookRouting,
};

/**
 * Providers whose rules can reroute the POST target: the override field is a
 * URL row rather than a channel or recipient list. Lives beside the behaviour
 * table because provider knowledge belongs here, not in the picker.
 */
export function routesByUrlOverride(integration: AlertIntegration): boolean {
  return (
    integration.provider_type === 'webhook' ||
    integration.provider_type === 'custom_webhook'
  );
}

const behaviourOf = (integration: AlertIntegration): RoutingBehaviour =>
  ROUTING_BEHAVIOUR[integration.provider_type];

/**
 * Which routing inputs a channel has to show, derived from the integration
 * rather than from the provider name alone.
 */
export function routingNeedsOf(integration: AlertIntegration): RoutingNeeds {
  return behaviourOf(integration).needs(integration);
}

/**
 * A stored `routing_override`, in the one-string-per-field shape the inputs
 * bind to.
 *
 * `integration` is optional because a rule can outlive the integration it
 * points at: the channel is still in the rule, and its override still has to
 * render rather than vanish while the user is looking at it.
 */
export function readRoutingOverride(
  integration: AlertIntegration | undefined,
  override: Record<string, unknown>,
): Record<string, string> {
  const read = integration ? behaviourOf(integration).read : stringFields;
  return read(override);
}

export function validateRoutingForIntegration(
  integration: AlertIntegration,
  routing: Record<string, string>,
  t: Translate,
): string | null {
  return behaviourOf(integration).validate(integration, routing, t);
}

/**
 * Every selected channel that fails its own routing rules, keyed by id.
 *
 * An id the caller selected but that no longer exists in `integrations` is
 * skipped rather than reported: it is not a routing mistake the user can fix
 * in this form.
 */
export function collectRoutingErrors(
  selectedIds: readonly number[],
  routingMap: RoutingMap,
  integrations: readonly AlertIntegration[],
  t: Translate,
): Record<number, string> {
  // Indexed once: the loop is over the selection and the lookup is over every
  // configured integration, so the pair is quadratic without it.
  const byId = new Map(integrations.map((i) => [i.id, i]));
  const errors: Record<number, string> = {};

  for (const id of selectedIds) {
    const integration = byId.get(id);
    if (!integration) continue;

    const error = validateRoutingForIntegration(
      integration,
      routingMap[id] ?? {},
      t,
    );
    if (error) errors[id] = error;
  }

  return errors;
}

/**
 * The `channels` array the API expects.
 *
 * Only the fields that provider actually routes on are sent: an empty
 * `routing_override` means "use the integration's own credentials", so
 * forwarding blank strings would overwrite a configured value with nothing.
 */
export function buildChannelsPayload(
  selectedIds: readonly number[],
  routingMap: RoutingMap,
  integrations: readonly AlertIntegration[],
): { integration_id: number; routing_override: Record<string, unknown> }[] {
  const byId = new Map(integrations.map((i) => [i.id, i]));

  return selectedIds.flatMap((id) => {
    const integration = byId.get(id);
    if (!integration) return [];

    return [
      {
        integration_id: id,
        routing_override: behaviourOf(integration).override(
          integration,
          routingMap[id] ?? {},
        ),
      },
    ];
  });
}
