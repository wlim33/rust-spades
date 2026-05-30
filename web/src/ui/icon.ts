import { html, nothing, type TemplateResult } from 'lit-html';
import { unsafeHTML } from 'lit-html/directives/unsafe-html.js';

// Vite inlines each vendored SVG's source at build time (no runtime fetch).
const raws = import.meta.glob('./icons/*.svg', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

const byName: Record<string, string> = {};
for (const [path, raw] of Object.entries(raws)) {
  const name = path.split('/').pop()!.replace('.svg', '');
  byName[name] = raw;
}

export function icon(
  name: string,
  opts: { label?: string; class?: string } = {},
): TemplateResult | typeof nothing {
  const raw = byName[name];
  if (!raw) return nothing;
  const cls = opts.class ? `icon ${opts.class}` : 'icon';
  return html`<span
    class=${cls}
    role=${opts.label ? 'img' : nothing}
    aria-label=${opts.label ?? nothing}
    aria-hidden=${opts.label ? nothing : 'true'}
    >${unsafeHTML(raw)}</span
  >`;
}
