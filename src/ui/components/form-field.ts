import { html, type TemplateResult } from 'lit-html';

export type FormFieldOpts = {
  id: string;
  label: string;
  value: string;
  onInput: (e: Event) => void;
  type?: 'text' | 'email' | 'password';
  placeholder?: string;
  autocomplete?: string;
  maxLength?: number;
  error?: string | null;
  disabled?: boolean;
};

export function formField(opts: FormFieldOpts): TemplateResult {
  return html`<div class="form-field">
    <label for=${opts.id}>${opts.label}</label>
    <input
      id=${opts.id}
      name=${opts.id}
      type=${opts.type ?? 'text'}
      .value=${opts.value}
      placeholder=${opts.placeholder ?? ''}
      autocomplete=${opts.autocomplete ?? 'off'}
      maxlength=${opts.maxLength ?? 200}
      ?disabled=${opts.disabled ?? false}
      @input=${opts.onInput}
    />
    ${opts.error
      ? html`<span data-testid="field-error" class="field-error">${opts.error}</span>`
      : null}
  </div>`;
}
