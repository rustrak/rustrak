import { createFileRoute, redirect } from '@tanstack/react-router';

/** Project settings open on General, the same way `/settings` opens on Profile. */
export const Route = createFileRoute('/_authenticated/projects/$id/settings/')({
  beforeLoad: ({ params }) => {
    throw redirect({ href: `/projects/${params.id}/settings/general` });
  },
});
