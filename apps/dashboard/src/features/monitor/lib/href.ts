/**
 * A monitor's page. Under `m/` because a slug is any `[a-z0-9_-]` word, so a
 * static sibling such as `crons/new` would otherwise shadow the monitor of
 * that slug.
 */
export function monitorHref(projectId: number, slug: string): string {
  return `/projects/${projectId}/crons/m/${encodeURIComponent(slug)}`;
}

export function monitorsHref(projectId: number): string {
  return `/projects/${projectId}/crons`;
}

export function newMonitorHref(projectId: number): string {
  return `${monitorsHref(projectId)}/new`;
}

export function editMonitorHref(projectId: number, slug: string): string {
  return `${monitorHref(projectId, slug)}/edit`;
}
