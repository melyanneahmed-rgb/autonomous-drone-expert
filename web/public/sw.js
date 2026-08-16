const CACHE_PREFIX = "smart-configurator-shell-";
const requestedVersion = new URL(self.location.href).searchParams.get("version");
const BUILD_VERSION = /^[0-9a-f]{7,64}$/.test(requestedVersion ?? "")
  ? requestedVersion
  : "local-development";
const CACHE_NAME = `${CACHE_PREFIX}${BUILD_VERSION}`;
const APP_BASE_URL = new URL("./", self.registration.scope);
const APP_SHELL = [
  APP_BASE_URL.href,
  new URL("manifest.webmanifest", APP_BASE_URL).href,
  new URL("favicon.svg", APP_BASE_URL).href,
];

function isWithinScope(url) {
  return (
    url.origin === APP_BASE_URL.origin &&
    url.pathname.startsWith(APP_BASE_URL.pathname)
  );
}

function isMutableRuntimeAsset(url) {
  const relativePath = url.pathname.slice(APP_BASE_URL.pathname.length);
  return (
    relativePath.startsWith("wasm/") ||
    relativePath === "manifest.webmanifest" ||
    relativePath === "favicon.svg"
  );
}

async function networkFirst(request, fallbackKey = request) {
  const cache = await caches.open(CACHE_NAME);
  try {
    const response = await fetch(request);
    if (response.ok) await cache.put(fallbackKey, response.clone());
    return response;
  } catch (error) {
    const cached = await cache.match(fallbackKey);
    if (cached) return cached;
    throw error;
  }
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    Promise.all([
      caches.open(CACHE_NAME).then((cache) => cache.addAll(APP_SHELL)),
      self.skipWaiting(),
    ]),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((name) => name.startsWith(CACHE_PREFIX) && name !== CACHE_NAME)
            .map((name) => caches.delete(name)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("message", (event) => {
  if (event.data?.type !== "CACHE_RESOURCES") return;
  const urls = Array.isArray(event.data.urls)
    ? [...new Set(event.data.urls)].filter((value) => {
        try {
          return isWithinScope(new URL(value));
        } catch {
          return false;
        }
      })
    : [];
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      .then((cache) => cache.addAll(urls))
      .then(() =>
        event.ports[0]?.postMessage({
          type: "CACHE_RESOURCES_COMPLETE",
          version: BUILD_VERSION,
        }),
      ),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (!isWithinScope(url)) return;

  if (request.mode === "navigate") {
    event.respondWith(networkFirst(request, APP_BASE_URL.href));
    return;
  }

  if (isMutableRuntimeAsset(url)) {
    event.respondWith(networkFirst(request));
    return;
  }

  event.respondWith(
    caches.match(request).then(
      (cached) =>
        cached ||
        fetch(request).then((response) => {
          if (response.ok) {
            const copy = response.clone();
            void caches.open(CACHE_NAME).then((cache) => cache.put(request, copy));
          }
          return response;
        }),
    ),
  );
});
