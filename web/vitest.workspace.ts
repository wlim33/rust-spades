import { defineWorkspace } from 'vitest/config';

export default defineWorkspace([
  {
    test: {
      name: 'unit',
      include: ['tests/unit/**/*.spec.ts'],
      environment: 'node',
    },
  },
  {
    define: {
      __BUILD_VERSION__: JSON.stringify('test'),
    },
    test: {
      name: 'component',
      include: ['tests/component/**/*.spec.ts'],
      environment: 'happy-dom',
      setupFiles: ['./happydom.setup.ts'],
    },
  },
]);
