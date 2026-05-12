import navaid from 'navaid';

export type RouteContext = {
  path: string;
  search: URLSearchParams;
};

export type RouteModule<P extends Record<string, string> = Record<string, string>> = {
  render: (params: P, ctx: RouteContext) => () => void;
};

type Routes = Record<string, RouteModule>;

export type Router = {
  handle: (path: string) => void;
  listen: () => void;
};

export function createRouter(routes: Routes): Router {
  // Node-test fallback for the full path; in the browser we read `location`.
  let pendingPath = '/';
  const currentFullPath = (): string => {
    if (typeof location !== 'undefined' && typeof location.pathname === 'string') {
      return location.pathname + location.search;
    }
    return pendingPath;
  };

  const r = navaid('/', (uri) => {
    const mod = routes['*'];
    if (!mod) return;
    const wild = (uri ?? '').replace(/^\//, '');
    runRoute(mod, { wild }, currentFullPath());
  });

  let currentCleanup: (() => void) | null = null;

  function runRoute(mod: RouteModule, params: Record<string, string>, fullPath: string): void {
    if (currentCleanup) currentCleanup();
    const search = new URLSearchParams(
      fullPath.includes('?') ? fullPath.slice(fullPath.indexOf('?')) : '',
    );
    currentCleanup = mod.render(params, { path: fullPath, search });
  }

  for (const [pattern, mod] of Object.entries(routes)) {
    if (pattern === '*') continue;
    r.on(pattern, (params) => {
      runRoute(mod, (params ?? {}) as Record<string, string>, currentFullPath());
    });
  }

  return {
    handle: (path: string) => {
      pendingPath = path;
      r.run(path);
    },
    listen: () => {
      r.listen();
    },
  };
}
