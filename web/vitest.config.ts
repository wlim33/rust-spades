import { defineConfig } from 'vitest/config';

// Vitest 4 removed `vitest.workspace.ts`/`defineWorkspace`; multi-project
// setups now live under `test.projects`. Each entry is self-contained (it does
// not inherit vite.config.ts), matching the old workspace behaviour.
export default defineConfig({
  test: {
    projects: [
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
    ],
  },
});
