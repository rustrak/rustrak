import { defineConfig } from '@rustrak/plugin';

export default defineConfig({
  name: 'Web Traffic',
  rustrak: '>=0.33.0',
  permissions: {
    traffic: { scope: 'project', actions: ['read'] },
    // Visitors one by one (their day, their profile): more private than
    // the figures, so it is given on its own.
    visitors: { scope: 'project', actions: ['read'] },
  },
  settings: {
    allowedDomains: { type: 'text', scope: 'project' },
    keepQuery: { type: 'boolean', scope: 'project', default: false },
    respectDoNotTrack: { type: 'boolean', default: true },
  },
});
