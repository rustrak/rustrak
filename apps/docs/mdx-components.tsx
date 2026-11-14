import { useMDXComponents as getDocsMDXComponents } from 'nextra-theme-docs';
import type { ComponentProps } from 'react';
import { InteractiveDemo } from './src/components/interactive-demo';

const FILE_RE = /^\/[^#?]*\.[a-z0-9]+(?:[#?].*)?$/i;

// Nextra's `a` routes every internal href through next/link, so `/llms.txt`
// or `/openapi.json` became a client navigation to a route that does not
// exist. A static file needs a real page load.
const DefaultA = getDocsMDXComponents().a;

function A(props: ComponentProps<'a'>) {
  if (typeof props.href === 'string' && FILE_RE.test(props.href)) {
    return (
      <a
        {...props}
        className="x:text-primary-600 x:underline x:hover:no-underline x:decoration-from-font x:[text-underline-position:from-font]"
      />
    );
  }
  return <DefaultA {...props} />;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const useMDXComponents = (components: any = {}) =>
  getDocsMDXComponents({ a: A, A, ...components, InteractiveDemo });
