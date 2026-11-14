/// <reference types="vite/client" />

// `@types/react-syntax-highlighter` declares the Prism grammars under
// `dist/cjs` only; the ESM ones are the same `any`.
declare module 'react-syntax-highlighter/dist/esm/languages/prism/*' {
  const language: unknown;
  export default language;
}
