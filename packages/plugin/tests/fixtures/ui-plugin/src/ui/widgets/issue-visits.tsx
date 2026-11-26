import { defineWidget, type WidgetProps } from '@rustrak/plugin/ui';
import { useState } from 'react';

export const config = defineWidget({ zone: 'issue.details.after' });

export default function IssueVisits({
  issue,
}: WidgetProps<'issue.details.after'>) {
  const [n] = useState(3);
  return <span>{`${issue.id}:${n}`}</span>;
}
