import { RoadmapPage } from '@/components/roadmap/roadmap-page';

/** A sibling of the landing, outside the docs theme for the same reasons. */
export const metadata = {
  title: 'Roadmap',
  description:
    'What the Rustrak team is building now, what comes next and what is still being explored.',
};

export default function Page() {
  return <RoadmapPage />;
}
