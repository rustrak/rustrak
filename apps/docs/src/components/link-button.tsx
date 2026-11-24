import { Button, type ButtonProps } from '@rustrak/ui';

/**
 * The design system's button as a link. Astro cannot hand a React element to
 * `render`, so this does it; rendered at build time, it ships no JavaScript.
 */
export function LinkButton({
  href,
  ...props
}: { href: string } & Omit<ButtonProps, 'render'>) {
  // react-doctor-disable-next-line react-doctor/anchor-has-content
  return <Button {...(props as ButtonProps)} render={<a href={href} />} />;
}
