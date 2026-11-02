'use client';

import dynamic from 'next/dynamic';
import '@scalar/api-reference-react/style.css';

const ApiReferenceReact = dynamic(
  () => import('@scalar/api-reference-react').then((m) => m.ApiReferenceReact),
  { ssr: false },
);

export default function ApiReferencePage() {
  const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '';

  return (
    <ApiReferenceReact
      configuration={{
        url: `${basePath}/openapi.json`,
        hideDownloadButton: false,
      }}
    />
  );
}
