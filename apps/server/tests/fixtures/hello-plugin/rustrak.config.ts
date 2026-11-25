import { defineConfig } from '@rustrak/plugin';

export default defineConfig({
  name: 'Hello',
  rustrak: '>=0.33.0',
  icon: 'star',
  permissions: {
    stats: { scope: 'project', actions: ['read'] },
    hits: { scope: 'project', actions: ['read', 'delete'] },
  },
  settings: {
    greeting: { type: 'text', default: 'Hello' },
    token: { type: 'secret' },
    maxPathLength: {
      type: 'number',
      scope: 'project',
      integer: true,
      default: 200,
      min: 10,
      max: 2000,
    },
  },
});
