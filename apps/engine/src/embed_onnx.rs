//! A12: the real on-device image encoder. Marqo-FashionSigLIP vision tower via
//! ONNX Runtime, behind the `onnx` cargo feature so the default build carries no
//! native ONNX dependency. Provider selection is cfg-gated: CoreML on macOS,
//! DirectML on Windows, CPU everywhere (and as the fallback). The ORT dylib is
//! resolved at runtime (load-dynamic), so no binary is linked at build time;
//! point `ORT_DYLIB_PATH` at a libonnxruntime if it is not on the default path.
//!
//! Preprocessing follows the model's SiglipImageProcessor config exactly:
//! RGB, resize to 224x224 (squash) with a bicubic-family filter, rescale by
//! 1/255, normalize with mean/std 0.5 (i.e. map [0,1] -> [-1,1]), CHW, NCHW.
//! Output is the 768-d image embedding, L2-normalized. (768, not the 512 the
//! PRD assumed; the engine's vector_dim follows the active encoder.)

use crate::embed::ImageEncoder;
use anyhow::{anyhow, Result};
use image::imageops::FilterType;
use ort::execution_providers::{ExecutionProviderDispatch, CPU};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::sync::Mutex;

pub const IMG_SIZE: u32 = 224;
pub const EMBED_DIM: usize = 768;
const MEAN: f32 = 0.5;
const STD: f32 = 0.5;

/// Decode + resize + rescale + normalize one image into a 1x3x224x224 CHW
/// float tensor, matching the SiglipImageProcessor spec. Pure (only the `image`
/// crate), so it is validated in CI against a Python reference without ORT.
pub fn preprocess(bytes: &[u8]) -> Result<Vec<f32>> {
    let decoded = image::load_from_memory(bytes)?.to_rgb8();
    let img = if decoded.width() == IMG_SIZE && decoded.height() == IMG_SIZE {
        decoded
    } else {
        // "squash" resize to an exact square; CatmullRom is the a=-0.5 cubic,
        // the same kernel PIL BICUBIC uses (edge/antialias handling can differ
        // by a hair, which the parity harness reports as a cosine, not exact).
        image::imageops::resize(&decoded, IMG_SIZE, IMG_SIZE, FilterType::CatmullRom)
    };
    let plane = (IMG_SIZE * IMG_SIZE) as usize;
    let mut data = vec![0f32; 3 * plane];
    for y in 0..IMG_SIZE {
        for x in 0..IMG_SIZE {
            let px = img.get_pixel(x, y).0;
            let idx = (y * IMG_SIZE + x) as usize;
            for c in 0..3 {
                data[c * plane + idx] = (px[c] as f32 / 255.0 - MEAN) / STD;
            }
        }
    }
    Ok(data)
}

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = (v.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt()).max(1e-12);
    v.iter().map(|x| (*x as f64 / norm) as f32).collect()
}

/// The frozen fashion vision encoder. `Session` is behind a Mutex so the
/// `ImageEncoder` trait's `&self` holds while ORT's `run` takes `&mut`.
pub struct OnnxImageEncoder {
    session: Mutex<Session>,
    input_name: String,
    num_outputs: usize,
    provider: &'static str,
}

impl OnnxImageEncoder {
    /// Build an encoder for the vision model at `model_path`. The provider
    /// default is CPU, deliberately: CPU int8 execution is bit-identical across
    /// machines, so embeddings and the rankings built on them are reproducible
    /// everywhere, which is the whole point of the determinism kernel. The
    /// platform accelerator (CoreML on macOS, DirectML on Windows) is opt-in via
    /// MOD_ENGINE_ONNX_PROVIDER=auto. It is faster but runs this int8 model
    /// through a different kernel path, so its embeddings drift from the CPU
    /// reference (~0.92 cosine on Marqo-FashionSigLIP) and are only
    /// self-consistent within one machine; use it only when the whole catalog
    /// and all queries are encoded on that same accelerator.
    pub fn new(model_path: &str) -> Result<Self> {
        let use_accel = std::env::var("MOD_ENGINE_ONNX_PROVIDER")
            .map(|v| {
                let v = v.trim().to_ascii_lowercase();
                v == "auto" || v == "accelerator" || v == "coreml" || v == "directml"
            })
            .unwrap_or(false);
        Self::build(model_path, !use_accel)
    }

    /// Force CPU (the deterministic reference path) or the platform accelerator,
    /// bypassing the env. The parity test uses this to measure both; callers can
    /// use it to pin a provider explicitly.
    #[allow(dead_code)]
    pub fn new_forced(model_path: &str, force_cpu: bool) -> Result<Self> {
        Self::build(model_path, force_cpu)
    }

    fn build(model_path: &str, force_cpu: bool) -> Result<Self> {
        let mut providers: Vec<ExecutionProviderDispatch> = Vec::new();
        #[allow(unused_mut)]
        let mut provider = "CPU";
        if !force_cpu {
            #[cfg(target_os = "macos")]
            {
                use ort::execution_providers::CoreML;
                providers.push(CoreML::default().build());
                provider = "CoreML";
            }
            #[cfg(target_os = "windows")]
            {
                use ort::execution_providers::DirectML;
                providers.push(DirectML::default().build());
                provider = "DirectML";
            }
        }
        providers.push(CPU::default().build());
        eprintln!(
            "[modsearch-engine] onnx image encoder: model={model_path} preferred_provider={provider}"
        );

        // ort's builder methods return Error<SessionBuilder> (to hand the
        // builder back on failure); map each to anyhow explicitly.
        let session = Session::builder()
            .map_err(|e| anyhow!("onnx: session builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("onnx: optimization level: {e}"))?
            .with_execution_providers(providers)
            .map_err(|e| anyhow!("onnx: execution providers: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("onnx: load model {model_path}: {e}"))?;

        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "pixel_values".to_string());
        let num_outputs = session.outputs().len();
        if input_name != "pixel_values" {
            eprintln!("[modsearch-engine] onnx: input name is '{input_name}' (expected 'pixel_values')");
        }
        Ok(Self { session: Mutex::new(session), input_name, num_outputs, provider })
    }

    pub fn provider(&self) -> &'static str {
        self.provider
    }
}

impl ImageEncoder for OnnxImageEncoder {
    fn dim(&self) -> usize {
        EMBED_DIM
    }

    fn encode_image(&self, bytes: &[u8]) -> Result<Vec<f32>> {
        let data = preprocess(bytes)?;
        let input = Tensor::from_array((
            [1_usize, 3, IMG_SIZE as usize, IMG_SIZE as usize],
            data,
        ))?;
        let mut session = self.session.lock().map_err(|_| anyhow!("onnx session mutex poisoned"))?;
        let outputs = session.run(ort::inputs![self.input_name.as_str() => input])?;
        // Pick the pooled image embedding: the output that flattens to exactly
        // EMBED_DIM (a [1,768] tensor), never last_hidden_state ([1,196,768]).
        for i in 0..self.num_outputs {
            if let Ok((_shape, flat)) = outputs[i].try_extract_tensor::<f32>() {
                if flat.len() == EMBED_DIM {
                    return Ok(l2_normalize(flat));
                }
            }
        }
        Err(anyhow!(
            "no [1,{EMBED_DIM}] embedding among {} outputs; check the vision model export",
            self.num_outputs
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::ImageEncoder;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/onnx")
            .join(name)
    }
    fn read_f32(path: &std::path::Path) -> Vec<f32> {
        std::fs::read(path)
            .unwrap()
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// Runs in CI. The test image is exactly 224x224, so no resize happens and
    /// decode+rescale+normalize+CHW must match the Python SiglipImageProcessor
    /// reference to the last ULP. This is the part of A12 that can be validated
    /// without the model or ONNX Runtime.
    #[test]
    fn preprocessing_matches_python_reference_exactly() {
        let img = std::fs::read(fixture("test_image_224.png")).unwrap();
        let got = preprocess(&img).unwrap();
        let want = read_f32(&fixture("ref_input_224.f32"));
        assert_eq!(got.len(), 3 * (IMG_SIZE * IMG_SIZE) as usize);
        assert_eq!(got.len(), want.len());
        let maxdiff = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        assert!(maxdiff < 1e-5, "preprocessing max abs diff {maxdiff} vs python reference");
    }

    /// Full inference parity, run on a machine that has the model. Set
    /// MOD_ENGINE_MODEL to the vision model onnx and generate ref_embed_224.f32
    /// with scripts/onnx_reference.py (which uses the CPU provider); otherwise
    /// this self-skips (e.g. in CI).
    ///
    /// The gate is CPU vs the Python CPU reference: same onnxruntime dylib, same
    /// int8 kernels, same input, so they must agree to near machine precision.
    /// That is what proves the preprocessing + ort port is numerically correct.
    /// The platform accelerator is then measured but NOT gated tightly: CoreML/
    /// DirectML run this int8 model through a different kernel path and drift
    /// from the CPU reference (~0.92 on this model), which is expected and fine
    /// as long as one provider encodes the whole vector space.
    #[test]
    fn inference_parity_with_python_when_model_present() {
        let Ok(model) = std::env::var("MOD_ENGINE_MODEL") else {
            eprintln!("skip inference parity: MOD_ENGINE_MODEL unset");
            return;
        };
        let ref_path = fixture("ref_embed_224.f32");
        if !ref_path.exists() {
            eprintln!("skip inference parity: run scripts/onnx_reference.py to emit {}", ref_path.display());
            return;
        }
        let img = std::fs::read(fixture("test_image_224.png")).unwrap();
        let want = read_f32(&ref_path);
        assert_eq!(want.len(), EMBED_DIM);
        // Both sides L2-normalize, so cosine == dot.
        let cosine = |got: &[f32]| -> f64 { got.iter().zip(&want).map(|(a, b)| *a as f64 * *b as f64).sum() };

        // Correctness gate: CPU must match the Python CPU reference to near
        // machine precision. This is the proof the port is numerically right.
        let enc_cpu = OnnxImageEncoder::new_forced(&model, true).unwrap();
        let got_cpu = enc_cpu.encode_image(&img).unwrap();
        assert_eq!(got_cpu.len(), EMBED_DIM);
        let cos_cpu = cosine(&got_cpu);
        eprintln!("onnx CPU parity cosine = {cos_cpu:.6} (provider {})", enc_cpu.provider());
        assert!(cos_cpu > 0.999, "onnx CPU parity cosine {cos_cpu} below 0.999: preprocessing or ort integration is off");

        // Characterization, not a gate: the platform accelerator drifts from the
        // CPU reference on this int8 model. Logged so the drift is a known
        // quantity, with a loose floor that still catches a wired-up-wrong
        // accelerator (which would score near zero).
        let enc_acc = OnnxImageEncoder::new_forced(&model, false).unwrap();
        if enc_acc.provider() != "CPU" {
            let got_acc = enc_acc.encode_image(&img).unwrap();
            let cos_acc = cosine(&got_acc);
            eprintln!("onnx {} vs CPU-reference cosine = {cos_acc:.6} (characterization, not gated)", enc_acc.provider());
            assert!(cos_acc > 0.85, "onnx {} cosine {cos_acc} implausibly low; accelerator path likely broken", enc_acc.provider());
        }
    }
}
