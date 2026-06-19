import navaid from 'navaid';

export type RouteContext = {
  path: string;
  search: URLSearchParams;
};

export type RouteModule<P extends Record<string, string> = Record<string, string>> = {
  render: (params: P, ctx: RouteContext) => () => void;
};

/** A route entry is either an eager module or a loader that code-splits it. */
export type RouteLoader<P extends Record<string, string> = Record<string, string>> = () => Promise<
  RouteModule<P>
>;

type RouteEntry = RouteModule | RouteLoader;
type Routes = Record<string, RouteEntry>;

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
    const entry = routes['*'];
    if (!entry) return;
    const wild = (uri ?? '').replace(/^\//, '');
    void runRoute(entry, { wild }, currentFullPath());
  });

  let currentCleanup: (() => void) | null = null;
  // Bumped on every navigation so a slow lazy-route load can detect that a newer
  // navigation superseded it and bail without mounting.
  let nav = 0;

  async function runRoute(
    entry: RouteEntry,
    params: Record<string, string>,
    fullPath: string,
  ): Promise<void> {
    const myNav = ++nav;
    const search = new URLSearchParams(
      fullPath.includes('?') ? fullPath.slice(fullPath.indexOf('?')) : '',
    );
    // Eager modules resolve synchronously; a loader code-splits its own chunk.
    const mod = typeof entry === 'function' ? await entry() : entry;
    if (myNav !== nav) return; // a newer navigation started while the chunk loaded
    // Tear down the previous route only once the next is ready — no blank gap
    // while a lazy chunk is in flight.
    if (currentCleanup) currentCleanup();
    currentCleanup = mod.render(params, { path: fullPath, search });
  }

  for (const [pattern, entry] of Object.entries(routes)) {
    if (pattern === '*') continue;
    r.on(pattern, (params) => {
      void runRoute(entry, (params ?? {}) as Record<string, string>, currentFullPath());
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
