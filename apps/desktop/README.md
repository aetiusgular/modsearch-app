# @aurasearch/desktop

The AuraSearch desktop client. Tauri 2.x (Rust core + webview frontend) with a single Rust sidecar
that hosts the on-device ML. Base Tauri app is ~5 MB; the ML lives in the sidecar, NOT a Python/torch
process (ADR-0001, PRD Section 6).

## Layout (planned)

```
crates/
  sidecar/     Rust ML worker: ONNX Runtime embeddings (Marqo-FashionSigLIP, CoreML/DirectML EP),
               embedded vector index (usearch/LanceDB), item-attribute graph (dict/CSR), and cheap
               idle-on-AC head training. This is the aura-recs-engine port target.
src-tauri/     Tauri app crate (to be created with `create-tauri-app`).
src/           webview frontend (to be added).
```

## Why one Rust sidecar and not Python

Shipping torch/peft cross-platform is not viable (2-3 GB, ONNX Runtime training is dead for Mac/Win).
The device runs frozen ONNX embeddings and trains only cheap heads locally; real encoder/GNN training
happens in the cloud (aurasearch-server). See PRD Section 6 and aura-recs-engine/ADAPTATION.md.

## Status

Stub. The sidecar crate compiles as an empty skeleton with module placeholders. The Tauri app itself
is not initialized yet; run `create-tauri-app` here when implementation begins.
