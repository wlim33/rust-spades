import { html, type TemplateResult } from 'lit-html';
import { toast } from '../../state/toast';

export function toastStack(): TemplateResult {
  return html`<div class="toast-stack" aria-live="polite">
    ${toast.toasts.value.map(
      (t) =>
        html`<div class=${`toast toast--${t.kind}`} data-testid="toast">
          <span>${t.message}</span>
          <button
            type="button"
            class="toast__close"
            aria-label="Dismiss"
            @click=${() => toast.dismiss(t.id)}
          >
            ×
          </button>
        </div>`,
    )}
  </div>`;
}
