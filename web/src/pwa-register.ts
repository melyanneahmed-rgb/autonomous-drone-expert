declare const __ADE_BUILD_SHA__: string;

function workerHasBuild(worker: ServiceWorker | null): worker is ServiceWorker {
  if (!worker || worker.state !== "activated") return false;
  return new URL(worker.scriptURL).searchParams.get("version") === __ADE_BUILD_SHA__;
}

async function waitForVersionedWorker(
  registration: ServiceWorkerRegistration,
): Promise<ServiceWorker> {
  if (workerHasBuild(registration.active)) return registration.active;

  return new Promise<ServiceWorker>((resolve, reject) => {
    const watched = new Set<ServiceWorker>();
    const timeout = window.setTimeout(() => {
      cleanup();
      reject(new Error("VERSIONED_SERVICE_WORKER_TIMEOUT"));
    }, 15_000);

    function cleanup(): void {
      window.clearTimeout(timeout);
      registration.removeEventListener("updatefound", check);
      navigator.serviceWorker.removeEventListener("controllerchange", check);
      for (const worker of watched) {
        worker.removeEventListener("statechange", check);
      }
    }

    function check(): void {
      if (workerHasBuild(registration.active)) {
        const active = registration.active;
        cleanup();
        resolve(active);
        return;
      }
      for (const worker of [registration.installing, registration.waiting]) {
        if (worker && !watched.has(worker)) {
          watched.add(worker);
          worker.addEventListener("statechange", check);
        }
      }
    }

    registration.addEventListener("updatefound", check);
    navigator.serviceWorker.addEventListener("controllerchange", check);
    check();
  });
}

async function cacheResourcesForOffline(
  worker: ServiceWorker,
  urls: string[],
): Promise<void> {
  const channel = new MessageChannel();
  await new Promise<void>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      channel.port1.close();
      reject(new Error("SERVICE_WORKER_CACHE_TIMEOUT"));
    }, 15_000);
    channel.port1.addEventListener(
      "message",
      (event: MessageEvent) => {
        window.clearTimeout(timeout);
        channel.port1.close();
        if (
          event.data?.type === "CACHE_RESOURCES_COMPLETE" &&
          event.data?.version === __ADE_BUILD_SHA__
        ) {
          resolve();
        } else {
          reject(new Error("SERVICE_WORKER_CACHE_ACK_INVALID"));
        }
      },
      { once: true },
    );
    channel.port1.start();
    worker.postMessage(
      { type: "CACHE_RESOURCES", urls },
      [channel.port2],
    );
  });
}

export function registerPwa(): void {
  document.documentElement.dataset.buildSha = __ADE_BUILD_SHA__;
  if (!("serviceWorker" in navigator)) return;

  async function enableOfflineShell(): Promise<void> {
    try {
      const baseUrl = new URL(import.meta.env.BASE_URL, window.location.origin);
      const workerUrl = new URL("sw.js", baseUrl);
      workerUrl.searchParams.set("version", __ADE_BUILD_SHA__);
      const existingRegistration = await navigator.serviceWorker.getRegistration(baseUrl.href);
      if (existingRegistration && !workerHasBuild(existingRegistration.active)) {
        await existingRegistration.unregister();
      }
      const registration = await navigator.serviceWorker.register(workerUrl, {
        scope: baseUrl.pathname,
        updateViaCache: "none",
      });
      const activeWorker = await waitForVersionedWorker(registration);
      const performanceUrls = performance
        .getEntriesByType("resource")
        .map((entry) => entry.name);
      const documentUrls = [...document.querySelectorAll("script[src], link[href]")].map(
        (element) =>
          element instanceof HTMLScriptElement ? element.src : (element as HTMLLinkElement).href,
      );
      const resourceUrls = [...new Set([...performanceUrls, ...documentUrls])]
        .filter((entryUrl) => {
          const url = new URL(entryUrl);
          return (
            url.origin === baseUrl.origin &&
            url.pathname.startsWith(baseUrl.pathname)
          );
        });
      await cacheResourcesForOffline(activeWorker, resourceUrls);
      document.documentElement.dataset.pwaStatus = "ready";
    } catch (error) {
      document.documentElement.dataset.pwaStatus =
        error instanceof Error ? error.message : "PWA_REGISTRATION_FAILED";
      // Offline installation is optional; the visible product must remain usable without it.
    }
  }

  void enableOfflineShell();
}
