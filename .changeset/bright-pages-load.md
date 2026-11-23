---
"@rustrak/server": minor
---

Web vitals from the browser SDK (LCP, INP, CLS, FCP and TTFB) are stored per page load and scored as Sentry scores them, with what each one was about: the LCP element and resource, the shifted element, the interaction target, the device and the network. Six new `/api/projects/{project_id}/web-vitals` endpoints read them per page, device, browser and element, and `@rustrak/client` gains `webVitals`. Browser projects get a Web Vitals page with the Core Web Vitals verdict, where to start, the pages ranked by what they cost, and a page per vital with how to fix it and its worst page loads.
