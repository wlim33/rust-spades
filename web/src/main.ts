import './ui/fonts.css';
import './ui/tokens.css';
import './ui/design.css';
import { createRouter } from './router';
import { home } from './routes/home';
import { create } from './routes/create';
import { login } from './routes/login';
import { signup } from './routes/signup';
import { oauthComplete } from './routes/oauth-complete';
import { settings } from './routes/settings';
import { profile } from './routes/profile';
import { leaderboard } from './routes/leaderboard';
import { notFound } from './routes/notfound';
import { session } from './state/session';
import { consumeOauthInProgress } from './lib/storage';
import { installGlobalErrorHandlers } from './lib/global-errors';
import { themeState } from './state/theme';

void (async () => {
  // Install the safety nets first: nothing below should be able to blank the
  // page with an unhandled error before the handlers exist.
  installGlobalErrorHandlers();
  // Local-only: when ?chaos is in the URL, monkeypatch fetch/WebSocket to inject
  // network faults before any connection is opened. Dynamically imported so the
  // chaos module is tree-shaken out of production builds entirely.
  if (import.meta.env.DEV) {
    await import('./lib/chaos').then((m) => m.installChaos());
  }
  themeState.initTheme();
  // If the user just returned from an OAuth provider, we may need to detour to
  // the username-picker (server set __oauth_pending: /auth/me 401s while we hold
  // a recent oauth-in-progress marker).
  const oauthMarker = consumeOauthInProgress();

  const router = createRouter({
    '/': home,
    '/create': create,
    // Lazy: the play route pulls in the card orchestrator, game-view, lobby and
    // boot — kept out of the initial bundle so the landing page doesn't pay for
    // the whole game UI.
    '/play/:shortId': () => import('./routes/play').then((m) => m.play),
    '/replay/:id': () => import('./routes/replay').then((m) => m.replay),
    '/login': login,
    '/signup': signup,
    '/auth/oauth/complete': oauthComplete,
    '/me': settings,
    '/u/:username': profile,
    '/leaderboard': leaderboard,
    '*': notFound,
  });

  // Delegate <a data-link> clicks to client-side navigation.
  // navaid's internal click handler intercepts all same-origin <a> tags, but we
  // use data-link as an explicit opt-in, so we handle it ourselves and let navaid
  // manage popstate via listen().
  document.addEventListener('click', (e) => {
    const target = (e.target as HTMLElement).closest('a[data-link]') as HTMLAnchorElement | null;
    if (!target) return;
    if (
      target.target === '_blank' ||
      e.metaKey ||
      e.ctrlKey ||
      e.altKey ||
      e.shiftKey ||
      e.button !== 0
    )
      return;
    const url = new URL(target.href);
    if (url.origin !== location.origin) return;
    e.preventDefault();
    history.pushState(null, '', url.pathname + url.search);
  });

  // Hydrate the session in the background — don't block first paint on an
  // /auth/me round-trip, and never let it reject boot. Auth-dependent UI reacts
  // to the currentUser signal; auth-gated routes await session.hydrated.
  const refreshing = session.refresh();

  // The OAuth detour is the one path that must know the result first: with a
  // recent oauth marker and still signed out, the server is awaiting a username
  // pick, so detour to /auth/oauth/complete.
  if (oauthMarker) {
    await refreshing;
    if (session.currentUser.value === null) {
      try {
        sessionStorage.setItem('spades_oauth_lingering', '1');
      } catch {
        // ignore
      }
      history.replaceState(null, '', '/auth/oauth/complete');
    }
  }

  router.listen();
})().catch((err) => {
  // Boot should never reject (refresh is caught internally), but if some
  // unexpected failure escapes, show a recoverable message instead of leaving a
  // blank page — whatever failed, the user can at least reload.
  console.error('boot failed', err);
  const root = document.getElementById('root');
  if (root) {
    root.replaceChildren();
    const p = document.createElement('p');
    p.style.padding = '2rem';
    p.style.textAlign = 'center';
    p.textContent = 'Something went wrong starting Spades. Please reload the page.';
    root.append(p);
  }
});
