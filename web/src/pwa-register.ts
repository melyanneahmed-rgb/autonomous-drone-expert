export function registerPwa(): void {
  if (!("serviceWorker" in navigator)) return;

  async function enableOfflineShell(): Promise<void> {
    try {
      const registration = await navigator.serviceWorker.register("/sw.js");
      await navigator.serviceWorker.ready;
      const resourceUrls = performance
        .getEntriesByType("resource")
        .map((entry) => entry.name)
        .filter((entryUrl) => new URL(entryUrl).origin === window.location.origin);
      registration.active?.postMessage({
        type: "CACHE_RESOURCES",
        urls: resourceUrls,
      });
    } catch {
      // Offline installation is optional; the visible product must remain usable without it.
    }
  }

  void enableOfflineShell();
}
