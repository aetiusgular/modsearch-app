//! AuraSearch desktop sidecar.
//!
//! Hosts the on-device ML: frozen ONNX image embeddings, the embedded vector index, the
//! item-attribute graph, and cheap idle-on-AC head training. This crate is the target of the
//! aura-recs-engine local-first port (see that repo's ADAPTATION.md and AURASEARCH_PRD_v3.md
//! Section 6). No torch, ever, on the client.

/// ONNX Runtime encoder (Marqo-FashionSigLIP), CoreML EP on macOS, DirectML on Windows.
pub mod embedding {}

/// Embedded HNSW vector index (usearch or LanceDB). Replaces the engine's server Qdrant.
pub mod index {}

/// Item-attribute adjacency (dict/CSR) + Jaccard fusion. Ports the engine's graph module.
pub mod graph {}

/// Idle-on-AC head training: projection/logistic heads and graph-edge-weight updates. Cheap, CPU.
pub mod trainer {}
