//! A12 seam: the item encoder. `ItemEncoder` is the Protocol the engine's
//! `embedding/base.py` defines; A12 swaps `SyntheticEncoder` for ONNX Runtime
//! running Marqo-FashionSigLIP (CoreML/DirectML) as a config-level change.
//!
//! The synthetic encoder is deterministic and attribute-structured: each
//! namespaced attribute token gets a fixed pseudo-random unit direction
//! (seeded by the token's FNV-1a hash), an item is the weighted sum of its
//! attribute directions plus a small per-id component, L2-normalized. Items
//! sharing brand/category/era/color land measurably closer in cosine, so the
//! taste loop and fusion rank on real structure while the model is absent.

use crate::det::{fnv1a, DetRng};
use crate::model::Listing;
use crate::retrieval::attribute_set;

pub trait ItemEncoder {
    /// Embedding width; A12's ONNX encoder reports the model's width here.
    #[allow(dead_code)]
    fn dim(&self) -> usize;
    fn encode(&self, listing: &Listing) -> Vec<f32>;
}

/// The image-embedding seam (A12). The synthetic path above embeds a listing's
/// attributes; this path embeds a listing's actual photo through a frozen
/// vision encoder. Ingest uses this for any listing whose image bytes are
/// available (real catalogs, A16) and falls back to the attribute encoder
/// otherwise, so the fixture catalog (no photos) still ranks. The ONNX
/// implementation lives in `embed_onnx` behind the `onnx` feature.
#[allow(dead_code)]
pub trait ImageEncoder {
    /// Embedding width (Marqo-FashionSigLIP is 768-d, not the 512 the PRD
    /// claimed; the engine's `vector_dim` follows the active encoder).
    fn dim(&self) -> usize;
    /// Encode raw image bytes (PNG/JPEG/WebP) into an L2-normalized vector.
    fn encode_image(&self, bytes: &[u8]) -> anyhow::Result<Vec<f32>>;
}

pub struct SyntheticEncoder {
    pub dim: usize,
}

impl SyntheticEncoder {
    fn token_direction(&self, token: &str) -> Vec<f32> {
        let mut rng = DetRng::new(fnv1a(token.as_bytes()));
        // Uniform components in [-1, 1); direction-only use, so no need for a
        // true Gaussian. Deterministic per token forever.
        (0..self.dim)
            .map(|_| (rng.next_u64() as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32)
            .collect()
    }
}

impl ItemEncoder for SyntheticEncoder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn encode(&self, listing: &Listing) -> Vec<f32> {
        let mut acc = vec![0.0f64; self.dim];
        for token in attribute_set(listing) {
            let dir = self.token_direction(&token);
            for (a, d) in acc.iter_mut().zip(dir.iter()) {
                *a += *d as f64;
            }
        }
        // Small id component so identical attribute sets are near, not equal.
        let id_dir = self.token_direction(&format!("id:{}", listing.id));
        for (a, d) in acc.iter_mut().zip(id_dir.iter()) {
            *a += 0.3 * *d as f64;
        }
        let norm = acc.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-9);
        acc.iter().map(|x| (x / norm) as f32).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn dot(a: &[f32], b: &[f32]) -> f64 {
        a.iter().zip(b.iter()).map(|(x, y)| *x as f64 * *y as f64).sum()
    }

    #[test]
    fn encoder_is_deterministic_and_unit() {
        let enc = SyntheticEncoder { dim: 64 };
        let cat = fixtures::catalog();
        let a = enc.encode(&cat[0]);
        let b = enc.encode(&cat[0]);
        assert_eq!(a, b);
        assert!((dot(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn shared_attributes_pull_items_closer() {
        let enc = SyntheticEncoder { dim: 128 };
        let cat = fixtures::catalog();
        let anchor = &cat[0];
        let same_brand_avg: Vec<f64> = cat
            .iter()
            .filter(|l| l.id != anchor.id && l.brand == anchor.brand)
            .map(|l| dot(&enc.encode(anchor), &enc.encode(l)))
            .collect();
        let diff_brand_avg: Vec<f64> = cat
            .iter()
            .filter(|l| l.brand != anchor.brand && l.category != anchor.category)
            .map(|l| dot(&enc.encode(anchor), &enc.encode(l)))
            .collect();
        if !same_brand_avg.is_empty() && !diff_brand_avg.is_empty() {
            let same = same_brand_avg.iter().sum::<f64>() / same_brand_avg.len() as f64;
            let diff = diff_brand_avg.iter().sum::<f64>() / diff_brand_avg.len() as f64;
            assert!(same > diff, "shared-attribute cosine {same} must beat disjoint {diff}");
        }
    }
}
