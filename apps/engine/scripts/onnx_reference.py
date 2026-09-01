#!/usr/bin/env python3
"""A12 reference generator (run on a machine with network + the model).

Produces the golden vectors the Rust parity tests compare against:
  tests/fixtures/onnx/ref_input_224.f32  -- the preprocessed 1x3x224x224 tensor
  tests/fixtures/onnx/ref_embed_224.f32  -- the L2-normalized 768-d embedding

Preprocessing mirrors the model's SiglipImageProcessor config exactly:
RGB, resize 224x224 (squash) bicubic, /255, normalize mean/std 0.5 -> [-1,1].

Usage:
    pip install onnxruntime pillow numpy huggingface_hub
    # either point at a local model...
    python3 scripts/onnx_reference.py --model /path/to/vision_model.onnx
    # ...or let it fetch the int8 vision model from the Hub:
    python3 scripts/onnx_reference.py --download            # int8 (93 MB)
    python3 scripts/onnx_reference.py --download --variant fp32

Then validate the Rust side:
    MOD_ENGINE_MODEL=/path/to/vision_model.onnx \
      cargo test -p modsearch-engine --features onnx -- --nocapture inference_parity
"""
import argparse
import os
import sys

import numpy as np
import onnxruntime as ort
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, "..", "tests", "fixtures", "onnx")
MODEL_REPO = "Marqo/marqo-fashionSigLIP"


def preprocess(path: str) -> np.ndarray:
    """SiglipImageProcessor: RGB, resize 224 bicubic squash, /255, [-1,1], CHW."""
    img = Image.open(path).convert("RGB")
    if img.size != (224, 224):
        img = img.resize((224, 224), Image.BICUBIC)
    arr = np.asarray(img).astype(np.float32) / 255.0
    arr = (arr - 0.5) / 0.5
    return arr.transpose(2, 0, 1)[None].astype("<f4")  # 1,3,224,224


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", help="path to a local vision_model onnx")
    ap.add_argument("--download", action="store_true", help="fetch from the Hub")
    ap.add_argument("--variant", default="int8", help="int8 | fp32 | fp16 | q4")
    args = ap.parse_args()

    model = args.model
    if not model and args.download:
        from huggingface_hub import hf_hub_download
        fname = "onnx/vision_model.onnx" if args.variant == "fp32" else f"onnx/vision_model_{args.variant}.onnx"
        model = hf_hub_download(MODEL_REPO, fname)
    if not model:
        print("give --model PATH or --download", file=sys.stderr)
        return 2

    img_path = os.path.join(FIX, "test_image_224.png")
    x = preprocess(img_path)
    x.tofile(os.path.join(FIX, "ref_input_224.f32"))
    print(f"preprocessed {img_path} -> {x.shape}  range[{x.min():.3f},{x.max():.3f}]")

    sess = ort.InferenceSession(model, providers=["CPUExecutionProvider"])
    in_name = sess.get_inputs()[0].name
    print("model:", os.path.basename(model), "| ort", ort.__version__)
    print("inputs :", [(i.name, i.shape) for i in sess.get_inputs()])
    print("outputs:", [(o.name, o.shape) for o in sess.get_outputs()])

    outs = sess.run(None, {in_name: x})
    emb = None
    for name, arr in zip([o.name for o in sess.get_outputs()], outs):
        flat = np.asarray(arr).reshape(-1)
        if flat.size == 768:
            emb = flat.astype(np.float32)
            print(f"picked embedding output '{name}' (768-d)")
            break
    if emb is None:
        print("no 768-d output found; outputs were:", [np.asarray(o).shape for o in outs], file=sys.stderr)
        return 3

    emb = emb / (np.linalg.norm(emb) + 1e-12)
    emb.astype("<f4").tofile(os.path.join(FIX, "ref_embed_224.f32"))
    print(f"wrote ref_embed_224.f32  (768-d, unit norm, first3={emb[:3]})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
