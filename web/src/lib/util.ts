export const API_URL: string = (import.meta.env.VITE_API_URL as string | undefined) ?? '';

export function navigateTo(path: string): void {
  if (typeof history !== 'undefined') {
    history.pushState(null, '', path);
  }
}
