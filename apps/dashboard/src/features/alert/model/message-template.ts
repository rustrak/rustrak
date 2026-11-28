/**
 * What the Custom Webhook's message-body editor offers the reader.
 *
 * No provider shapes live in code paths. Rustrak posts whatever body you write
 * and judges the delivery by the HTTP status, the way Grafana's Custom Payload,
 * Uptime Kuma's Custom Body and Alertmanager's webhook receiver all do. The
 * shapes particular services expect are starting points here, in
 * `TEMPLATE_PRESETS`, and the server renders each one in its tests so a
 * preset that would hand someone a broken body fails a test instead.
 *
 * Fields are written bare: `{{ issue.title }}`. The server escapes a value
 * for wherever it sits, inside a string or as a value of its own, so nobody
 * has to learn a filter before their first alert works.
 */

/**
 * Where the field's help button goes: the page that documents the body
 * template on its own, rather than a paragraph in the alerts page.
 */
export const TEMPLATE_DOCS_URL =
  'https://rustrak.dev/usage/alerts/message-body-template';

/**
 * The editor's placeholder: a minimal body, not copy. Held here rather than
 * in the message dictionaries because braces are configuration, and ICU
 * would read them as argument markers.
 */
export const templatePlaceholder = '{"text": "Rustrak: {{ issue.title }}"}';

/** Which part of the payload a field belongs to; the insert menu groups by it. */
export type TemplateVariableGroup = 'issue' | 'project' | 'alert';

/**
 * A payload field the editor can insert.
 */
export interface TemplateVariable {
  path: string;
  /** What insertion writes: the bare field, escaped by the server on render. */
  snippet: string;
  /** The rendered type, shown beside the name the way an editor shows one. */
  detail: string;
  /** Key under `alerts.customWebhook.variables`, one short line. */
  descriptionKey: string;
  /** What this field holds in the sample payload, so the list reads concretely. */
  example: string;
  group: TemplateVariableGroup;
}

const variable = (
  group: TemplateVariableGroup,
  path: string,
  detail: string,
  example: string,
): TemplateVariable => ({
  path,
  snippet: `{{ ${path} }}`,
  detail,
  descriptionKey: path,
  example,
  group,
});

/**
 * Every field of the alert payload, in the order the editor offers them: the
 * ones a message is actually built from first, the bookkeeping last.
 */
export const TEMPLATE_VARIABLES: readonly TemplateVariable[] = [
  variable('issue', 'issue.title', 'string', 'Sample issue'),
  variable('issue', 'issue.short_id', 'string', 'SAMPLE-1'),
  variable('issue', 'issue.level', 'string | null', 'error'),
  variable('issue', 'issue.event_count', 'number', '1'),
  variable(
    'issue',
    'issue_url',
    'string',
    'https://rustrak.example/issues/sample',
  ),
  variable(
    'issue',
    'issue.id',
    'string',
    '00000000-0000-0000-0000-000000000000',
  ),
  variable('issue', 'issue.first_seen', 'timestamp', '2026-01-01T00:00:00Z'),
  variable('issue', 'issue.last_seen', 'timestamp', '2026-01-01T00:00:00Z'),
  variable('project', 'project.name', 'string', 'Sample Project'),
  variable('project', 'project.slug', 'string', 'sample-project'),
  variable('project', 'project.id', 'number', '1'),
  variable('alert', 'alert_type', 'string', 'new_issue'),
  variable(
    'alert',
    'alert_id',
    'string',
    '00000000-0000-0000-0000-000000000000',
  ),
  variable('alert', 'triggered_at', 'timestamp', '2026-01-01T00:00:00Z'),
  variable('alert', 'actor', 'string', 'Rustrak'),
];

/** The groups in display order, each with its i18n key under `customWebhook.groups`. */
export const TEMPLATE_VARIABLE_GROUPS: readonly TemplateVariableGroup[] = [
  'issue',
  'project',
  'alert',
];

const KNOWN_PATHS = new Set(TEMPLATE_VARIABLES.map((v) => v.path));

/** Whether `path` names a field of the alert payload. */
export function isKnownVariable(path: string): boolean {
  return KNOWN_PATHS.has(path);
}

/**
 * A body to start from. `name` is the service's own name and is not
 * translated; the one-line description is, under `customWebhook.presets`.
 *
 * Bodies are stored indented, the way the reader would have written them,
 * because they land in the editor as-is.
 */
export interface TemplatePreset {
  id: string;
  name: string;
  /** Key under `alerts.customWebhook.presets`. */
  descriptionKey: string;
  body: string;
}

const preset = (id: string, name: string, body: string): TemplatePreset => ({
  id,
  name,
  descriptionKey: id,
  body,
});

/**
 * Mirrored in the server's `custom_webhook.rs` tests (`PRESET_*`), rendered
 * against a payload with a quoted, multi-line title and no level. Change one
 * side, change the other.
 */
export const TEMPLATE_PRESETS: readonly TemplatePreset[] = [
  preset(
    'minimal',
    'Minimal',
    `{
  "text": "Rustrak: {{ issue.title }} ({{ issue.short_id }})\\n{{ issue_url }}"
}`,
  ),
  preset(
    'slack',
    'Slack-compatible',
    `{
  "text": "*{{ issue.title }}* ({{ issue.short_id }})\\n{{ issue_url }}"
}`,
  ),
  preset(
    'mattermost',
    'Mattermost',
    `{
  "attachments": [
    {
      "fallback": "{{ issue.title }} ({{ issue.short_id }})",
      "color": "#d24b4e",
      "title": "{{ issue.title }}",
      "title_link": "{{ issue_url }}",
      "text": "{{ project.name }} · {{ issue.short_id }} · {{ issue.event_count }} events"
    }
  ]
}`,
  ),
  preset(
    'discord',
    'Discord',
    `{
  "content": "**{{ issue.title }}** ({{ issue.short_id }})\\n{{ issue_url }}"
}`,
  ),
  preset(
    'teams',
    'Microsoft Teams',
    `{
  "type": "message",
  "attachments": [
    {
      "contentType": "application/vnd.microsoft.card.adaptive",
      "content": {
        "type": "AdaptiveCard",
        "version": "1.4",
        "body": [
          {
            "type": "TextBlock",
            "text": "{{ issue.title }}",
            "weight": "Bolder",
            "size": "Medium",
            "wrap": true
          },
          {
            "type": "TextBlock",
            "text": "{{ project.name }} · {{ issue.short_id }} · {{ issue.event_count }} events",
            "isSubtle": true,
            "wrap": true
          }
        ],
        "actions": [
          {
            "type": "Action.OpenUrl",
            "title": "View issue",
            "url": "{{ issue_url }}"
          }
        ]
      }
    }
  ]
}`,
  ),
  preset(
    'wecom',
    'WeCom',
    `{
  "msgtype": "text",
  "text": {
    "content": "Rustrak: {{ issue.title }} ({{ issue.short_id }})\\n{{ issue_url }}"
  }
}`,
  ),
  preset(
    'wecomMarkdown',
    'WeCom markdown',
    `{
  "msgtype": "markdown",
  "markdown": {
    "content": "### {{ issue.title }}\\n> Project: {{ project.name }}\\n> Level: {{ issue.level }}\\n> [View issue]({{ issue_url }})"
  }
}`,
  ),
  preset(
    'dingtalk',
    'DingTalk',
    `{
  "msgtype": "text",
  "text": {
    "content": "Rustrak: {{ issue.title }} ({{ issue.short_id }})\\n{{ issue_url }}"
  }
}`,
  ),
  preset(
    'feishu',
    'Feishu / Lark',
    `{
  "msg_type": "text",
  "content": {
    "text": "Rustrak: {{ issue.title }} ({{ issue.short_id }})\\n{{ issue_url }}"
  }
}`,
  ),
  preset(
    'structured',
    'Structured JSON',
    `{
  "source": "rustrak",
  "event": {{ alert_type }},
  "severity": {{ issue.level }},
  "title": {{ issue.title }},
  "url": {{ issue_url }},
  "occurrences": {{ issue.event_count }},
  "project": {{ project.slug }}
}`,
  ),
];
