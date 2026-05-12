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
  // Track the current full path (including query string) for each handle() call.
  let pendingPath = '/';

  const r = navaid('/', (uri) => {
    // navaid's wildcard handler — we get the unmatched pathname (no query string).
    // navaid formats it with a leading slash (e.g. "/nope"); strip it for the params.
    const mod = routes['*'];
    if (!mod) return;
    const wild = (uri ?? '').replace(/^\//, '');
    runRoute(mod, { wild }, pendingPath);
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
      runRoute(mod, (params ?? {}) as Record<string, string>, pendingPath);
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
