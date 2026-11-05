import type { CatalogResource, Permission, SetAccess } from '@rustrak/client';

/** A role of the list, the person's own permissions, or nothing chosen yet. */
export type RoleChoice = number | 'personal' | null;

/** One line of someone's access: what they hold, and where. */
export interface AccessRow {
  role: RoleChoice;
  /** `null`: every project, including ones created later. */
  projects: number[] | null;
}

type Held = {
  role_id: number;
  project_id?: number | null;
  is_personal: boolean;
};

export function rowsFrom(assignments: Held[]): AccessRow[] {
  const rows: AccessRow[] = [];
  for (const a of assignments) {
    const role = a.is_personal ? 'personal' : a.role_id;
    const row = rows.find((r) => r.role === role);
    const project = a.project_id ?? null;
    if (!row) {
      rows.push({ role, projects: project === null ? null : [project] });
    } else if (project === null) {
      row.projects = null;
    } else if (row.projects) {
      row.projects.push(project);
    }
  }
  return rows;
}

export function toSetAccess(
  rows: AccessRow[],
  personalPermissions: Permission[],
): SetAccess {
  const personal = rows.find((r) => r.role === 'personal');
  return {
    assignments: rows.flatMap((row) =>
      typeof row.role !== 'number'
        ? []
        : (row.projects ?? [null]).map((project_id) => ({
            role_id: row.role as number,
            project_id,
          })),
    ),
    personal: personal
      ? { permissions: personalPermissions, project_ids: personal.projects }
      : null,
  };
}

export function sameAccess(a: SetAccess, b: SetAccess): boolean {
  const canonical = (access: SetAccess) => ({
    assignments: access.assignments
      .map((r) => `${r.role_id}:${r.project_id ?? ''}`)
      .sort(),
    projects: access.personal?.project_ids?.slice().sort() ?? null,
    permissions: access.personal?.permissions
      .map((p) => `${p.resource}:${p.action}`)
      .sort(),
  });
  return JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
}

/** Whether where they are held matters: workspace permissions apply anyway. */
export const reachesProjects = (
  permissions: Permission[],
  catalog: CatalogResource[],
): boolean =>
  permissions.some(
    (p) => catalog.find((r) => r.key === p.resource)?.scope === 'project',
  );
