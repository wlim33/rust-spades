import { describe, it, expect, beforeEach } from 'vitest';
import { createRouter, type RouteModule } from '../../src/router';

describe('createRouter', () => {
  let calls: string[];
  let cleanups: string[];

  const makeRoute = (name: string): RouteModule<Record<string, string>> => ({
    render: (params) => {
      calls.push(`${name}(${JSON.stringify(params)})`);
      return () => {
        cleanups.push(name);
      };
    },
  });

  beforeEach(() => {
    calls = [];
    cleanups = [];
  });

  it('calls the matching route render with params', () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '/u/:name': makeRoute('profile'),
    });
    router.handle('/u/alice');
    expect(calls).toEqual(['profile({"name":"alice"})']);
  });

  it('runs the previous route cleanup before mounting the next', () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '/u/:name': makeRoute('profile'),
    });
    router.handle('/');
    router.handle('/u/bob');
    expect(cleanups).toEqual(['home']);
    expect(calls).toEqual(['home({})', 'profile({"name":"bob"})']);
  });

  it('falls back to the wildcard route for unknown paths', () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '*': makeRoute('notfound'),
    });
    router.handle('/nope');
    expect(calls).toEqual(['notfound({"wild":"nope"})']);
  });

  it('passes search params via second argument', () => {
    const seen: string[] = [];
    const router = createRouter({
      '/login': {
        render: (_p, ctx) => {
          seen.push(ctx.search.get('next') ?? '');
          return () => {};
        },
      },
    });
    router.handle('/login?next=/me');
    expect(seen).toEqual(['/me']);
  });

  it('awaits a lazy route loader and tears down the previous route only after it resolves', async () => {
    const router = createRouter({
      '/': makeRoute('home'),
      '/play': () => Promise.resolve(makeRoute('play')),
    });
    router.handle('/');
    router.handle('/play');
    // The loader is async: the previous route is still mounted, nothing new yet.
    expect(calls).toEqual(['home({})']);
    expect(cleanups).toEqual([]);
    await Promise.resolve();
    await Promise.resolve();
    expect(cleanups).toEqual(['home']);
    expect(calls).toEqual(['home({})', 'play({})']);
  });

  it('does not mount a lazy route that a newer navigation superseded', async () => {
    let resolvePlay!: (m: RouteModule) => void;
    const router = createRouter({
      '/': makeRoute('home'),
      '/play': () => new Promise<RouteModule>((r) => (resolvePlay = r)),
      '/u/:name': makeRoute('profile'),
    });
    router.handle('/play'); // starts loading (pending)
    router.handle('/u/zoe'); // supersedes before the chunk resolves
    resolvePlay(makeRoute('play'));
    await Promise.resolve();
    await Promise.resolve();
    // The stale play load must not mount over the newer profile route.
    expect(calls).toEqual(['profile({"name":"zoe"})']);
  });
});
