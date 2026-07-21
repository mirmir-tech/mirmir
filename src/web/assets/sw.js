const cacheName = "mirmir-dashboard-shell-v7";
const shellPaths = [
  "/ui/",
  "/ui/app.css?v=5",
  "/ui/app.js?v=7",
  "/ui/assets/brand/lockup.svg",
  "/ui/assets/brand/favicon.svg?v=2",
  "/ui/assets/brand/topography.svg",
  "/ui/assets/fonts/space-grotesk-latin.woff2",
  "/ui/assets/fonts/space-grotesk-latin-ext.woff2",
  "/ui/assets/fonts/inter-latin.woff2",
  "/ui/assets/fonts/inter-latin-ext.woff2",
  "/ui/assets/fonts/jetbrains-mono-latin.woff2",
  "/ui/assets/fonts/jetbrains-mono-latin-ext.woff2",
];
const shellPathnames = new Set(shellPaths.map((path) => new URL(path, self.location).pathname));

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(cacheName)
      .then((cache) => cache.addAll(shellPaths))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys()
      .then((names) => Promise.all(names
        .filter((name) => name.startsWith("mirmir-dashboard-shell-") && name !== cacheName)
        .map((name) => caches.delete(name))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== self.location.origin || !shellPathnames.has(url.pathname)) return;
  event.respondWith(networkFirst(request));
});

async function networkFirst(request) {
  const cache = await caches.open(cacheName);
  try {
    const response = await fetch(request);
    if (response.ok && response.headers.get("x-mirmir-dashboard") === "1") {
      await cache.put(request, response.clone());
      return response;
    }
    return await cache.match(request, { ignoreSearch: true }) || response;
  } catch (_error) {
    return await cache.match(request, { ignoreSearch: true })
      || new Response("MiRMiR dashboard is unavailable", { status: 503 });
  }
}
