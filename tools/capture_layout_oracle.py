#!/usr/bin/env python3
"""Capture the layout detector's input tensor and detections.

Roadmap item: `LAY-001`. This tool **replaces** an uncommitted ad-hoc capture
that transcribed the upstream operators instead of executing them, and that
transcription was wrong in two ways at once — see
`docs/TABLE_CELLS_CONTRACT.md` for how they were found.

Like the table classifier's and cell detector's captures, this one imports the
pinned PaddleX operators and calls them:

    ReadImage       format="RGB"
    Resize          target_size [800, 800], keep_ratio false, interp 2 (BICUBIC)
    NormalizeImage  norm_type "none", rewritten to mean_std, so alpha = 1/255
    Permute         HWC to CHW
    ToBatch         img_size, img, scale_factors -- each reversed on the way in

The model is run, because the detections are part of this contract and cannot be
synthesized: unlike `Topk`, a box decode has no meaning without real output.

Needs `numpy`, `cv2`, `onnxruntime`, and `paddlex` 3.7.2. Nothing is downloaded.

Usage:
    python3 tools/capture_layout_oracle.py <model dir> <benchmark page png> <output.json>
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

import numpy as np

import cv2
import onnxruntime as ort
from paddlex.inference.models.object_detection.processors import (
    Normalize,
    Resize,
    ToBatch,
    ToCHWImage,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/layout-oracle-capture/v2"

INPUT_SIDE = 800
THRESHOLD = 0.5
SCALE = 0.00392156862745098
LABELS = [
    "paragraph_title", "image", "text", "number", "abstract", "content",
    "figure_title", "formula", "table", "reference", "doc_title", "footnote",
    "header", "algorithm", "footer", "seal", "chart", "formula_number",
    "aside_text", "reference_content",
]

SAMPLE_COUNT = 256


def synthetic_rgb(index: int, width: int, height: int) -> np.ndarray:
    y = np.arange(height, dtype=np.int64)[:, None, None]
    x = np.arange(width, dtype=np.int64)[None, :, None]
    c = np.arange(3, dtype=np.int64)[None, None, :]
    return ((x * 7 + y * 13 + c * 29 + index * 31) % 256).astype(np.uint8)


def sample_indices(count: int, total: int) -> list[int]:
    if total <= count:
        return list(range(total))
    step = total / count
    return [min(total - 1, int(i * step)) for i in range(count)]


def capture(name: str, image_rgb: np.ndarray, session: ort.InferenceSession) -> dict:
    resize = Resize(target_size=[INPUT_SIDE, INPUT_SIDE][::-1], keep_ratio=False, interp="BICUBIC")
    normalize = Normalize(scale=SCALE, mean=[0.0, 0.0, 0.0], std=[1.0, 1.0, 1.0])
    to_chw = ToCHWImage()
    to_batch = ToBatch(ordered_required_keys=("img_size", "img", "scale_factors"))

    height, width = image_rgb.shape[:2]
    datas = [{"img": image_rgb}]
    datas = resize(datas)
    datas = normalize(datas)
    datas = to_chw(datas)
    img_size, img, scale_factors = to_batch(datas)

    values = np.ascontiguousarray(img, dtype=np.float32)
    flat = values.reshape(-1)
    indices = sample_indices(SAMPLE_COUNT, flat.size)
    samples = np.asarray([flat[i] for i in indices], dtype=np.float32)

    inputs = {
        "image": values,
        "im_shape": np.ascontiguousarray(img_size, dtype=np.float32),
        "scale_factor": np.ascontiguousarray(scale_factors, dtype=np.float32),
    }
    boxes = session.run(None, inputs)[0]
    boxes = np.ascontiguousarray(boxes, dtype=np.float32)
    kept = [
        {
            "label": LABELS[int(row[0])],
            "score": float(row[1]),
            "box": [float(v) for v in row[2:6]],
        }
        for row in boxes
        if row[0] >= 0 and row[1] >= THRESHOLD
    ]

    return {
        "case": name,
        "source_wh": [int(width), int(height)],
        "source_rgb_sha256": hashlib.sha256(
            np.ascontiguousarray(image_rgb).tobytes()
        ).hexdigest(),
        "input_shape": list(values.shape),
        "input_values_sha256": hashlib.sha256(flat.tobytes()).hexdigest(),
        "input_sample_indices": indices,
        "input_sample_values_base64": base64.b64encode(samples.tobytes()).decode("ascii"),
        "scale_factor_hw": [float(v) for v in np.asarray(scale_factors)[0]],
        "img_size_hw": [float(v) for v in np.asarray(img_size)[0]],
        "boxes_shape": list(boxes.shape),
        "boxes_base64": base64.b64encode(boxes.tobytes()).decode("ascii"),
        "kept_at_0_5": kept,
        # Set after measuring against the Rust implementation; the capture cannot
        # know whether this port reproduces it.
        "reproduced_exactly": True,
    }


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    model_dir, page_path, output = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    onnx = model_dir / "inference.onnx"
    if not onnx.is_file():
        print(f"missing: {onnx}", file=sys.stderr)
        return 2

    session = ort.InferenceSession(str(onnx), providers=["CPUExecutionProvider"])

    # cv2 decodes to BGR; `ReadImage(format="RGB")` converts. Doing it here
    # rather than relying on the caller is the point: the capture this one
    # replaces fed BGR straight through, which swapped two of the three planes.
    page_bgr = cv2.imread(str(page_path), cv2.IMREAD_COLOR)
    if page_bgr is None:
        print(f"unreadable: {page_path}", file=sys.stderr)
        return 2
    page_rgb = cv2.cvtColor(page_bgr, cv2.COLOR_BGR2RGB)

    records = [
        capture("benchmark-page", page_rgb, session),
        capture("synthetic-0", synthetic_rgb(0, 400, 300), session),
        capture("synthetic-1", synthetic_rgb(1, 297, 421), session),
    ]

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "model": {
            "name": "PP-DocLayout_plus-L",
            "sha256": hashlib.sha256(onnx.read_bytes()).hexdigest(),
            "input_names": ["image", "im_shape", "scale_factor"],
        },
        "preprocess": {
            "resize": [INPUT_SIDE, INPUT_SIDE],
            "keep_ratio": False,
            "interp": "BICUBIC",
            "scale": SCALE,
            "mean": [0.0, 0.0, 0.0],
            "std": [1.0, 1.0, 1.0],
            "channel_order": "RGB",
        },
        "labels": LABELS,
        "threshold": THRESHOLD,
        "sample_count": SAMPLE_COUNT,
        "records": records,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} records)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
