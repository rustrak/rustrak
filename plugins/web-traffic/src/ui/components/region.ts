import { useLocale } from '@rustrak/plugin/ui';
import { useMemo } from 'react';

/** A country code as its name in the reader's language. */
export function useRegionName(): (code: string) => string {
  const locale = useLocale();
  return useMemo(() => {
    const names = new Intl.DisplayNames([locale], { type: 'region' });
    return (code: string) => {
      try {
        return names.of(code) ?? code;
      } catch {
        return code;
      }
    };
  }, [locale]);
}
