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
  // No public sourcemaps: they ship to Cloudflare Pages (~3× the JS size) and
  // expose the full un-minified source. Use 'hidden' instead if an error
  // tracker is wired up later (emits maps to upload, without referencing them).
  build: { outDir: 'dist', sourcemap: false },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/games': { target: 'http://localhost:3000', changeOrigin: true, ws: true },
      '/matchmaking': { target: 'http://localhost:3000', changeOrigin: true },
      '/challenges': { target: 'http://localhost:3000', changeOrigin: true },
      '/auth': { target: 'http://localhost:3000', changeOrigin: true },
      '/users': { target: 'http://localhost:3000', changeOrigin: true },
      '/player': { target: 'http://localhost:3000', changeOrigin: true },
    },
  },
  define: {
    __BUILD_VERSION__: JSON.stringify(buildVersion),
  },
});
