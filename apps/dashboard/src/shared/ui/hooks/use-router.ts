import { useNavigate } from '@tanstack/react-router';
import { useMemo } from 'react';

/** Navigation by `href`, for the call sites that build one. */
export function useRouter() {
  const navigate = useNavigate();

  return useMemo(
    () => ({
      push: (href: string) => {
        void navigate({ href });
      },
      replace: (href: string) => {
        void navigate({ href, replace: true });
      },
    }),
    [navigate],
  );
}
