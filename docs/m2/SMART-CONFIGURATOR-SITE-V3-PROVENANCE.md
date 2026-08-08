# Smart Configurator Site v3 provenance boundary

## Accepted reference

| Field | Verified value |
| --- | --- |
| Product identity | Smart Configurator / Autonomous Drone Expert |
| Saved Site version | 3 |
| Source commit | `4d6dbc801f67c79bfe172ded9a819e42d084fdc7` |
| Tracked files | 35 |
| Recovered ZIP SHA-256 | `07637fb1ec448fc2c9184f2acc30af8ad708d0d09e5e834b888c4cd8772b48b1` |
| Source manifest SHA-256 | `f96cd4c0279bfb08750a5c9226cb0660d6033c59249c43d68f5e5aa031c692ae` |
| Recovery report SHA-256 | `2cc3d5d7ab900a3379ae889b18984d3d70ef1764420f861fb9caa36a04756b80` |
| Site package-lock SHA-256 | `283dbdf55081ff6e460baff80764f39f722f13a0720e1a5fa13153ea877051a5` |
| Verification date | 2026-08-08 (Europe/Amsterdam) |

The ZIP, manifest and recovery report were supplied by the owner and verified read-only.
The 35 exported files match the recovered Site v3 commit. No other repository was used.

## What is approved

The Site is the approved design and interaction reference: Arabic RTL-first presentation,
responsive flow, product identity, explicit states and risk communication. Its source may
inform a later selective port after the repository package and dependency lock are approved.

## What is not imported by this gate

- no TSX, CSS, service worker, asset or UI component;
- no `.openai/hosting.json`, Worker, Vinext or Cloudflare integration;
- no Next App Router or React Server Components;
- no Drizzle/D1/R2 database scaffolding or examples;
- no Tailwind/PostCSS scaffolding;
- no Web Serial chooser, firmware file flow or storage implementation;
- no ZIP, recovered manifest or platform-generated build output.

Site v3 contains browser APIs and scaffolding beyond the current implementation boundary.
Their presence in the reference is provenance evidence, not authorization to implement
them. WEBAPP remains the active product surface; APK remains deferred.
