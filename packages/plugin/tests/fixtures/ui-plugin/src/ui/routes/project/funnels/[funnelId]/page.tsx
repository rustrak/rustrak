import type { PageProps } from '@rustrak/plugin/ui';

export default function Funnel({ params }: PageProps<'project'>) {
  return <p>{`funnel ${params.funnelId}`}</p>;
}
