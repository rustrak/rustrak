import { definePage } from '@rustrak/plugin/ui';

export const config = definePage({
  sidebar: { label: 'nav.overview', icon: 'star' },
});

export default function Overview() {
  return <p>everywhere</p>;
}
