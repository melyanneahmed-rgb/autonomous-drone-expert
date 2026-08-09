import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

test("approved UI port map pins source authority and every selective destination", () => {
  const map = fs.readFileSync(
    new URL("../../docs/m2/APPROVED-UI-V3-PORT-MAP.md", import.meta.url),
    "utf8",
  );
  assert.match(map, /4d6dbc801f67c79bfe172ded9a819e42d084fdc7/);
  assert.match(map, /07637fb1ec448fc2c9184f2acc30af8ad708d0d09e5e834b888c4cd8772b48b1/);
  for (const mapping of [
    ["app/page.tsx", "web/src/App.tsx"],
    ["app/globals.css", "web/src/styles.css"],
    ["app/layout.tsx", "web/index.html"],
    ["app/pwa-register.tsx", "web/src/pwa-register.ts"],
    ["public/manifest.webmanifest", "web/public/manifest.webmanifest"],
    ["public/sw.js", "web/public/sw.js"],
    ["public/favicon.svg", "web/public/favicon.svg"],
  ]) {
    assert.ok(mapping.every((value) => map.includes(`\`${value}\``)), `missing mapping: ${mapping.join(" -> ")}`);
  }
  assert.match(map, /FUNCTIONAL CAPABILITY DEFERRED \/ VISUAL DESIGN PRESERVED/);
  assert.match(map, /Visible design deviations: 0/);
});
