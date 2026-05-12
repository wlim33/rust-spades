import './ui/design.css';
import { createRouter } from './router';
import { home } from './routes/home';
import { notFound } from './routes/notfound';

const router = createRouter({
  '/': home,
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

router.listen();
