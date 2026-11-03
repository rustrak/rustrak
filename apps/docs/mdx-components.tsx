import { useMDXComponents as getDocsMDXComponents } from 'nextra-theme-docs';
import { InteractiveDemo } from './src/components/interactive-demo';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const useMDXComponents = (components: any = {}) =>
  getDocsMDXComponents({ ...components, InteractiveDemo });
