import { defineConfig } from 'vite';
import { execSync } from 'node:child_process';

const buildVersion = (() => {
  try {
    return execSync('git rev-parse --short HEAD').toString().trim();
  } catch {
    return 'dev';
  }
})();

export default defineConfig({
  base: '/',
  build: { outDir: 'dist', sourcemap: true },
  server: { port: 5173, strictPort: true },
  define: {
    __BUILD_VERSION__: JSON.stringify(buildVersion),
  },
});
