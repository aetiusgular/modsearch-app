# Running AuraSearch locally (A9 + A14 + A19)

This wires the app to the on-device engine over native messaging. No localhost:
the app loads from the extension, the engine is a native process.

## Prerequisites
- pnpm and Node
- Rust (stable)

## 1. Build the engine
```
cargo build --release -p aurasearch-engine
```
Binary: `target/release/aurasearch-engine` (note its absolute path).

## 2. Build the app and stage it into the extension
```
pnpm install
apps/extension/build.sh
```
This builds `apps/web` and copies the result to `apps/extension/app/`.

## 3. Load the extension
- Chrome → chrome://extensions → enable Developer mode → Load unpacked → select `apps/extension`.
- Copy the extension ID it shows.

## 4. Register the native host
```
apps/extension/host/install-host.sh "$(pwd)/target/release/aurasearch-engine" <EXTENSION_ID>
```
Restart Chrome so it picks up the host.

## 5. Open it
Click the AuraSearch toolbar icon. The full-page app opens and now talks to the
engine (real handlers over the stub catalog). Append `?mock` to the URL to force
the mock data client instead.

## Dev loop (UI only)
```
pnpm -C apps/web dev
```
Runs Vite for fast UI iteration. This uses the mock client and a dev server; it is
a developer convenience, never part of the shipped product. End users never touch
localhost.

## What is stubbed
The engine (A9) serves an in-memory synthetic catalog. A10 adds SQLite/DuckDB, A11
the vector index + graph, A12 ONNX embedding, A13 the taste model, A16 real ingested
boutiques. See AURASEARCH_BUILD_PROMPTS.md.
