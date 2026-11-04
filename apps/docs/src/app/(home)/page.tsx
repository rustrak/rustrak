import { Home } from '@/components/home/home';

/**
 * The title deliberately does not claim parity with Sentry: `rustrak-brand`'s
 * voice guide rules that phrasing out by name. Compatibility is claimed where
 * it is true — the SDK tabs and the DSN line — and nowhere as parity.
 */
export const metadata = {
  title: 'Rustrak: error tracking on a server you own',
  description:
    'Self-hosted, Sentry-compatible error tracking. Point any official Sentry SDK at one binary you run yourself. Open source under AGPL-3.0.',
};

export default function Page() {
  return <Home />;
}
