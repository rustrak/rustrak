/** Someone an issue can be assigned to. */
export interface Assignable {
  id: number;
  email: string;
}

/**
 * Who the reader can assign an issue to: the project's members, and always
 * the reader -- an admin sees every project without being a member of it,
 * and an issue assigned to them must still read as theirs.
 */
export function assignable(
  members: readonly { user_id: number; email: string }[],
  reader: { id: number; email: string } | null,
): Assignable[] {
  const list = members.map((m) => ({ id: m.user_id, email: m.email }));
  if (reader && !list.some((m) => m.id === reader.id)) {
    list.unshift({ id: reader.id, email: reader.email });
  }
  return list;
}

/**
 * The members of several projects as one list, each person once: who a list
 * across projects can name as an assignee.
 */
export function everyone(lists: readonly Assignable[][]): Assignable[] {
  const byId = new Map<number, Assignable>();
  for (const person of lists.flat()) byId.set(person.id, person);
  return [...byId.values()];
}
