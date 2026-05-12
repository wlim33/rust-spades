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
});
