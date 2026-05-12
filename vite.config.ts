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
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/games': { target: 'http://localhost:3000', changeOrigin: true, ws: true },
      '/matchmaking': { target: 'http://localhost:3000', changeOrigin: true },
      '/challenges': { target: 'http://localhost:3000', changeOrigin: true },
    },
  },
  define: {
    __BUILD_VERSION__: JSON.stringify(buildVersion),
  },
});
