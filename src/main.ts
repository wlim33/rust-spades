import './ui/design.css';
import { createRouter } from './router';
import { home } from './routes/home';
import { create } from './routes/create';
import { play } from './routes/play';
import { login } from './routes/login';
import { signup } from './routes/signup';
import { notFound } from './routes/notfound';
import { session } from './state/session';
import { consumeOauthInProgress } from './lib/storage';

void (async () => {
  // Best-effort: hydrate the session before mounting the first route.
  // If the user just returned from an OAuth provider, detour to the
  // username-picker when the server set the __oauth_pending cookie (signaled
  // by /auth/me returning 401 while we have a recent oauth-in-progress marker).
  const oauthMarker = consumeOauthInProgress();
  await session.refresh();

  const router = createRouter({
    '/': home,
    '/create': create,
    '/play/:shortId': play,
    '/login': login,
    '/signup': signup,
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

  // If we have an oauth marker and we're NOT signed in, the server is awaiting
  // a username pick. Detour to /auth/oauth/complete (route added in Plan 3 Task 7).
  if (oauthMarker && session.currentUser.value === null) {
    history.replaceState(null, '', '/auth/oauth/complete');
  }

  router.listen();
})();
