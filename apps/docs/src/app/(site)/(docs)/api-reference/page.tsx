'use client';

import dynamic from 'next/dynamic';
import '@scalar/api-reference-react/style.css';

const ApiReferenceReact = dynamic(
  () => import('@scalar/api-reference-react').then((m) => m.ApiReferenceReact),
  { ssr: false },
);

export default function ApiReferencePage() {
  return (
    <ApiReferenceReact
      configuration={{
        url: '/openapi.json',
        hideDownloadButton: false,
      }}
    />
  );
}
