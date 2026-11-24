import { ApiReferenceReact } from '@scalar/api-reference-react';
import '@scalar/api-reference-react/style.css';

/**
 * Every endpoint, from the server's own OpenAPI document (`/openapi.json`,
 * the file `cargo test` keeps in step with the code). Scalar draws it and
 * lets the reader try requests; it only runs in the browser.
 */
export function ApiReference() {
  return (
    <ApiReferenceReact
      configuration={{
        url: '/openapi.json',
        hideDownloadButton: false,
        darkMode: true,
        forceDarkModeState: 'dark',
        hideDarkModeToggle: true,
        withDefaultFonts: false,
        // Scalar's own toolbar (Configure, Share, Deploy API) is for whoever
        // edits the spec, not for someone reading it.
        showDeveloperTools: 'never',
      }}
    />
  );
}
