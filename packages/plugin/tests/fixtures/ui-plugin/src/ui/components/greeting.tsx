import { usePluginTranslations } from '@rustrak/plugin/ui';

/** Not a route: components beside the routes are just components. */
export function Greeting() {
  const t = usePluginTranslations();
  return <b>{t('greeting', { name: 'Ana' })}</b>;
}
