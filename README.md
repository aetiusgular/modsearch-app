# aurasearch-app

The public client monorepo for AuraSearch, a local-first universal fashion search tool. Holds the browser extension and the desktop client, plus the shared wire contract they both depend on. Licensed GPL-3.0.

AuraSearch is a separate product from Agora (a Grailed-like second-hand marketplace). It piggybacks on Agora by reusing Agora's recommendation engine as a fork (`aura-recs-engine`), and may index Agora as one of its search sources. See `AURASEARCH_PRD_v3.md` for the full plan and `aura-recs-engine/docs/adr/0001-repository-architecture.md` for why the project is split into three repos.

## Layout

```
packages/
  contract/        shared wire contract (JSON Schema + generated TS types). Single source of truth.
apps/
  extension/       browser extension. One TS core, three builds: Chromium (Chrome+Edge), Firefox, Safari (separate app).
  desktop/         Tauri desktop client + Rust sidecar (ONNX embeddings, vector index, graph, on-device head training).
```

This is a polyglot monorepo. TypeScript is managed with a pnpm workspace (`pnpm-workspace.yaml`); Rust with a Cargo workspace (`Cargo.toml`). Native workspaces only, no Nx or Bazel until build times demand it.

## What lives where, and what does not

In this repo (open, GPL-3.0): the extension, the desktop client, the shared contract. All client-side, all shippable to users, all clean of secrets.

Not in this repo: the eBay OAuth proxy, affiliate service, adapter registry, cloud training, and any passive-capture adapters. Those live in the private `aurasearch-server` repo because they hold secrets and carry the project's legal exposure. Keeping them out is what lets this repo be open.

The recommendation engine lives in its own repo (`aura-recs-engine`, Apache-2.0) and is consumed here as a pinned dependency once the local-first port begins.

## Status

Scaffold only. No implementation yet. The contract package is real and mirrors the engine's schemas; everything under `apps/` is a stub with a README describing what goes there. See the PRD roadmap (Section 10) for build order.

## Getting started (once implementation begins)

```
pnpm install
pnpm -C packages/contract generate   # generate TS types from JSON Schema
```

Extension builds and the Tauri desktop app have their own READMEs under `apps/`.
