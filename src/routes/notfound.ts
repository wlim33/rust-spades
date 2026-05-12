import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import type { RouteModule } from '../router';

export const notFound: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    render(
      appShell(html`
        <h1>Not found</h1>
        <p><a href="/" data-link>Back home</a></p>
      `),
      root,
    );
    return () => render(html``, root);
  },
};
