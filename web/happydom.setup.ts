// happy-dom provides window/document globally. Component tests must never make
// real network calls: with API_URL='' a relative fetch resolves against vitest's
// default happy-dom origin (http://localhost:3000), where happy-dom opens a real
// socket. With no backend that connection is refused and the in-flight request
// leaks past window teardown as a noisy ECONNREFUSED/AbortError stack trace
// (e.g. home.render() -> startQueuePoll() -> fetch('/matchmaking/queue-sizes')).
//
// Stub fetch to fail fast and silently — this mirrors "backend unreachable",
// which every caller already tolerates (refreshQueueSizes, openSse both catch).
// A spec that needs a specific response overrides this with
// vi.stubGlobal('fetch', ...) and restores it via vi.unstubAllGlobals().
const offlineFetch: typeof fetch = () =>
  Promise.reject(new Error('fetch is disabled in component tests; stub it explicitly if needed'));
globalThis.fetch = offlineFetch;

export {};
