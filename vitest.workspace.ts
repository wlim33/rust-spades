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
    test: {
      name: 'component',
      include: ['tests/component/**/*.spec.ts'],
      environment: 'happy-dom',
      setupFiles: ['./happydom.setup.ts'],
    },
  },
]);
