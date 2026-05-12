import { html, type TemplateResult } from 'lit-html';

export type ButtonVariant = 'primary' | 'secondary' | 'danger';

export function button(opts: {
  label: string;
  onClick: (e: Event) => void;
  variant?: ButtonVariant;
  disabled?: boolean;
}): TemplateResult {
  const variant = opts.variant ?? 'primary';
  return html`<button
    type="button"
    class="btn btn--${variant}"
    ?disabled=${opts.disabled ?? false}
    @click=${opts.onClick}
  >
    ${opts.label}
  </button>`;
}
