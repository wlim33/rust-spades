import { signal } from '@preact/signals-core';

export type ToastKind = 'info' | 'error' | 'success';
export type Toast = { id: number; kind: ToastKind; message: string };

const toasts = signal<Toast[]>([]);
let nextId = 1;

function show(kind: ToastKind, message: string, ttlMs = 4000): void {
  const id = nextId++;
  toasts.value = [...toasts.value, { id, kind, message }];
  setTimeout(() => dismiss(id), ttlMs);
}

function dismiss(id: number): void {
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

export const toast = {
  toasts,
  info: (m: string) => show('info', m),
  error: (m: string) => show('error', m),
  success: (m: string) => show('success', m),
  dismiss,
};
