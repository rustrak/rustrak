/*!
 * Web Traffic, a Rustrak plugin: privacy-first page views without cookies.
 *
 *   <script defer src="https://rustrak.example.com/public/web-traffic/script.js"
 *     data-project="1" data-key="<the project's DSN public key>"></script>
 *
 *   webTraffic.track('signup', { plan: 'pro' });   // an event of your own
 *   <button data-traffic-event="buy">              // or one on a click
 *
 * data-auto-track="false" leaves page views to webTraffic.pageview().
 */
(() => {
  const script = document.currentScript;
  if (!script || window.webTraffic) return;
  const project = script.getAttribute('data-project');
  const key = script.getAttribute('data-key');
  if (!project || !key) return;
  // Served at /public/<plugin>/script.js: the plugin it reports to.
  const src = new URL(script.src);
  const plugin = src.pathname.split('/')[2];
  const endpoint = `${src.origin}/api/projects/${encodeURIComponent(project)}/plugins/${plugin}/collect?sentry_key=${encodeURIComponent(key)}`;

  let lastUrl = null;
  // The referrer belongs to a visit's first page: an SPA's later pages keep
  // the same document.referrer.
  let referrer = document.referrer || undefined;

  const send = (beacon) => {
    const body = JSON.stringify({ ...beacon, url: location.href });
    // A string beacon is text/plain: no CORS preflight, and it survives the
    // page being closed.
    if (navigator.sendBeacon?.(endpoint, body)) return;
    fetch(endpoint, {
      method: 'POST',
      body,
      keepalive: true,
      mode: 'no-cors',
    }).catch(() => {});
  };

  const pageview = () => {
    if (location.href === lastUrl) return;
    lastUrl = location.href;
    send({
      type: 'pageview',
      referrer,
      title: document.title || undefined,
      language: navigator.language || undefined,
    });
    referrer = undefined;
  };

  const track = (name, data) => {
    if (!name) return;
    send({
      type: 'event',
      name: String(name),
      data: data && typeof data === 'object' ? data : undefined,
    });
  };

  window.webTraffic = { pageview, track };

  document.addEventListener(
    'click',
    (e) => {
      const el = e.target?.closest?.('[data-traffic-event]');
      if (el) track(el.getAttribute('data-traffic-event'));
    },
    true,
  );

  if (script.getAttribute('data-auto-track') === 'false') return;
  // Single-page apps change pages without loading one.
  for (const method of ['pushState', 'replaceState']) {
    const original = history[method];
    history[method] = function (...args) {
      const result = original.apply(this, args);
      pageview();
      return result;
    };
  }
  addEventListener('popstate', pageview);
  if (document.visibilityState === 'prerender') {
    document.addEventListener('visibilitychange', pageview, { once: true });
  } else {
    pageview();
  }
})();
