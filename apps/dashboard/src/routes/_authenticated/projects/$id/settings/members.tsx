import { createFileRoute, redirect } from '@tanstack/react-router';

/** Who holds what moved to the team page: an old link still lands somewhere. */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/members',
)({
  beforeLoad: () => {
    throw redirect({ href: '/settings/team' });
  },
});
