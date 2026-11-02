/** Labels typed as `bug, needs triage`: trimmed, empty and repeated dropped. */
export function parseLabels(text: string): string[] {
  return [
    ...new Set(
      text
        .split(',')
        .map((label) => label.trim())
        .filter(Boolean),
    ),
  ];
}
