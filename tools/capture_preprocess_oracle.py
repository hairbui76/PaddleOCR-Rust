#!/usr/bin/env python3
"""Capture upstream detector and recognizer input tensors.

Roadmap item: `PRE-001`.

`PRE-001` asks that this port match *captured* upstream preprocessing tensors
within the declared tolerance, not that it merely reimplement the arithmetic and
check its own work. Everything before this tool was verified by construction:
`src/tensor.rs` was written from the pinned source and tested against
hand-derived expectations. That is a weaker claim, and the difference matters
precisely where a reimplementation is most likely to be wrong — the resize
kernel's fixed-point rounding and the order the scale, mean, and standard
deviation are applied in.

So this tool does not re-derive anything. It imports the upstream operator
classes from the read-only reference checkout and runs them, which makes the
capture the upstream computation rather than a second opinion about it:

    DetResizeForTest(limit_side_len=960, limit_type="max")
    NormalizeImage(scale="1./255.", mean=[0.485, 0.456, 0.406],
                   std=[0.229, 0.224, 0.225], order="hwc")
    ToCHWImage()

read from `tools/infer/predict_det.py:pre_process_list` and
`tools/infer/utility.py:init_args` at the pinned revision.

The recognizer side cannot be imported the same way: `resize_norm_img` is a
method on `TextRecognizer`, whose constructor builds a predictor. Its sequence is
therefore transcribed from `tools/infer/predict_rec.py:resize_norm_img` at the
pinned revision, with the `use_onnx` width override deliberately not applied
because the pinned export has a dynamic width axis, which upstream leaves alone.
Every numerically significant step is still the upstream call itself —
`cv2.resize` at its default `INTER_LINEAR`, the `float32` transpose then `/255`,
then `-0.5`, then `/0.5`, then the zero-padded `(C, H, W)` canvas — so what
differs from the detector side is who sequences the calls, not who performs them.

Nothing is written through the reference checkout; it is imported and read only.
The capture is emitted as JSON with the tensor stored as base64 `float32` in C
order, so the committed fixture is exact rather than decimal-rounded.

This tool is not part of the Rust build or test path. It needs `numpy`, `cv2`,
`PIL`, and `paddle` — the last only because `operators.py` imports it at module
scope — none of which this repository depends on.

The full capture is tens of megabytes and is deliberately not committed. The
tool also emits a compact fixture holding, per input, the tensor shape, the
SHA-256 of the exact `float32` little-endian C-order bytes, and a fixed stride
sample of exact values. The digest is the real check; the samples exist so a
failure says *where*, not merely *that*.

Usage:
    python3 tools/capture_preprocess_oracle.py <PaddleOCR checkout> \
        <full-capture.json> <compact-fixture.json>
"""

from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import sys
from pathlib import Path

import numpy as np

# The fixture inputs to capture, by directory name under tests/fixtures.
FIXTURES = [
    "classic-v1-e2e-reading-order",
    "classic-v1-e2e-unicode",
    "classic-v1-e2e-tall-crop",
    "classic-v1-benchmark-page",
]

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/classic-preprocess-oracle-capture/v1"

# Deterministic synthetic crops for the recognizer capture, in caller order.
# The set spans the narrow end, the base ratio, a fractional ratio, and enough
# crops to force two batches with different padded widths.
RECOGNIZER_CROP_SIZES = [
    (20, 48),
    (48, 48),
    (96, 48),
    (160, 48),
    (320, 48),
    (49, 47),
    (1, 240),
    (400, 48),
    (240, 50),
    # 503/50 = 10.06, so 48 * 10.06 = 482.88 — the widest crop in its batch, and
    # a batch width that is not a whole number before rounding. Upstream
    # truncates it; anything that rounds up instead produces a different tensor,
    # and only a fractional case that is also the batch maximum shows it.
    (503, 50),
]

# Read from tools/infer/utility.py:init_args and predict_rec.py.
RECOGNITION_SHAPE = (3, 48, 320)
RECOGNITION_BATCH_SIZE = 6
FIXTURE_SCHEMA_VERSION = "paddleocr-rust/classic-preprocess-detector-input/v1"

# How many exact elements the compact fixture keeps per input. A fixed count with
# a derived stride keeps every fixture the same size regardless of tensor size.
SAMPLE_COUNT = 512

# Read from tools/infer/utility.py:init_args at the pinned revision.
DET_LIMIT_SIDE_LEN = 960
DET_LIMIT_TYPE = "max"

# Read from tools/infer/predict_det.py:pre_process_list at the pinned revision.
NORMALIZE = {
    "scale": "1./255.",
    "mean": [0.485, 0.456, 0.406],
    "std": [0.229, 0.224, 0.225],
    "order": "hwc",
}


def load_upstream_operators(checkout: Path):
    """Imports `ppocr/data/imaug/operators.py` from the reference checkout."""
    path = checkout / "ppocr" / "data" / "imaug" / "operators.py"
    if not path.is_file():
        raise SystemExit(f"not found: {path}")
    # The module is loaded by path rather than by installing the package, so
    # nothing is written into the reference checkout and no __pycache__ is
    # created inside it.
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("upstream_operators", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def capture_one(operators, image_bgr: np.ndarray) -> dict:
    """Runs the upstream detector preprocessing chain over one BGR image."""
    resize = operators.DetResizeForTest(
        limit_side_len=DET_LIMIT_SIDE_LEN, limit_type=DET_LIMIT_TYPE
    )
    normalize = operators.NormalizeImage(**NORMALIZE)
    to_chw = operators.ToCHWImage()

    data = {"image": image_bgr.copy()}
    data = resize(data)
    if data is None:
        raise SystemExit("upstream resize rejected the image")
    resized_shape = list(np.asarray(data["image"]).shape)
    data = normalize(data)
    data = to_chw(data)

    tensor = np.asarray(data["image"], dtype=np.float32)
    tensor = np.expand_dims(tensor, axis=0)
    contiguous = np.ascontiguousarray(tensor, dtype="<f4")
    return {
        "resized_hwc_shape": resized_shape,
        "shape": list(contiguous.shape),
        "dtype": "float32",
        "order": "C",
        "values_base64": base64.b64encode(contiguous.tobytes()).decode("ascii"),
        "values_sha256": hashlib.sha256(contiguous.tobytes()).hexdigest(),
    }


def synthetic_crop(index: int, width: int, height: int) -> np.ndarray:
    """Builds one deterministic BGR crop.

    The formula is trivial on purpose: it has to be reproducible byte for byte
    in Rust, so anything depending on a random seed, a platform float, or a
    library version would defeat the point of the comparison.
    """
    x = np.arange(width, dtype=np.int64)[None, :, None]
    y = np.arange(height, dtype=np.int64)[:, None, None]
    c = np.arange(3, dtype=np.int64)[None, None, :]
    return ((x * 7 + y * 13 + c * 29 + index * 31) % 256).astype(np.uint8)


def upstream_resize_norm_img(image: np.ndarray, max_wh_ratio: float) -> np.ndarray:
    """Transcribed from `tools/infer/predict_rec.py:resize_norm_img`.

    The `use_onnx` width override is not applied: it only fires when the model's
    input width axis is a positive integer, and the pinned export leaves that
    axis dynamic, which upstream passes over.
    """
    import math

    import cv2

    channels, height, _ = RECOGNITION_SHAPE
    assert channels == image.shape[2]
    width = int(height * max_wh_ratio)
    source_height, source_width = image.shape[:2]
    ratio = source_width / float(source_height)
    if math.ceil(height * ratio) > width:
        resized_w = width
    else:
        resized_w = int(math.ceil(height * ratio))
    resized = cv2.resize(image, (resized_w, height))
    resized = resized.astype("float32")
    resized = resized.transpose((2, 0, 1)) / 255
    resized -= 0.5
    resized /= 0.5
    padded = np.zeros((channels, height, width), dtype=np.float32)
    padded[:, :, 0:resized_w] = resized
    return padded


def capture_recognizer_batches() -> list[dict]:
    """Runs the upstream recognition preprocessing over the synthetic crops."""
    _, height, base_width = RECOGNITION_SHAPE
    ratios = [width / float(crop_height) for width, crop_height in RECOGNIZER_CROP_SIZES]
    order = sorted(range(len(ratios)), key=lambda index: ratios[index])

    batches = []
    for start in range(0, len(order), RECOGNITION_BATCH_SIZE):
        chunk = order[start : start + RECOGNITION_BATCH_SIZE]
        max_wh_ratio = base_width / height
        for index in chunk:
            max_wh_ratio = max(max_wh_ratio, ratios[index])
        rows = []
        for index in chunk:
            width, crop_height = RECOGNIZER_CROP_SIZES[index]
            crop = synthetic_crop(index, width, crop_height)
            rows.append(upstream_resize_norm_img(crop, max_wh_ratio))
        tensor = np.ascontiguousarray(np.stack(rows, axis=0), dtype="<f4")
        batches.append(
            {
                "original_indices": chunk,
                "max_wh_ratio": max_wh_ratio,
                "shape": list(tensor.shape),
                "dtype": "float32",
                "order": "C",
                "values_base64": base64.b64encode(tensor.tobytes()).decode("ascii"),
                "values_sha256": hashlib.sha256(tensor.tobytes()).hexdigest(),
            }
        )
        print(
            f"recognizer batch {len(batches)}: rows {chunk}, "
            f"max_wh_ratio {max_wh_ratio:.6f}, shape {list(tensor.shape)}"
        )
    return batches


def compact_record(record: dict) -> dict:
    """Reduces one full capture record to the committed fixture form."""
    captured = record["detector_input"]
    values = np.frombuffer(
        base64.b64decode(captured["values_base64"]), dtype="<f4"
    )
    stride = max(1, values.size // SAMPLE_COUNT)
    indices = list(range(0, values.size, stride))[:SAMPLE_COUNT]
    return {
        "fixture_id": record["fixture_id"],
        "input_sha256": record["input_sha256"],
        "source_hwc_shape": record["source_hwc_shape"],
        "shape": captured["shape"],
        "resized_hwc_shape": captured["resized_hwc_shape"],
        "values_sha256": captured["values_sha256"],
        "sample_stride": stride,
        "sample_indices": indices,
        # Exact bits, not decimal text: a rounded sample would compare a
        # different number than the one upstream produced.
        "sample_values_base64": base64.b64encode(
            np.ascontiguousarray(values[indices], dtype="<f4").tobytes()
        ).decode("ascii"),
    }


def compact_batch(batch: dict) -> dict:
    """Reduces one recognizer batch to the committed fixture form."""
    values = np.frombuffer(base64.b64decode(batch["values_base64"]), dtype="<f4")
    stride = max(1, values.size // SAMPLE_COUNT)
    indices = list(range(0, values.size, stride))[:SAMPLE_COUNT]
    return {
        "original_indices": batch["original_indices"],
        "max_wh_ratio": batch["max_wh_ratio"],
        "shape": batch["shape"],
        "values_sha256": batch["values_sha256"],
        "sample_stride": stride,
        "sample_indices": indices,
        "sample_values_base64": base64.b64encode(
            np.ascontiguousarray(values[indices], dtype="<f4").tobytes()
        ).decode("ascii"),
    }


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    checkout = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2])
    fixture_output = Path(sys.argv[3])

    import cv2

    operators = load_upstream_operators(checkout)

    records = []
    for fixture in FIXTURES:
        png = Path("tests/fixtures") / fixture / "input.png"
        raw = png.read_bytes()
        image = cv2.imdecode(np.frombuffer(raw, dtype=np.uint8), cv2.IMREAD_COLOR)
        if image is None:
            raise SystemExit(f"cv2 could not decode {png}")
        record = {
            "fixture_id": fixture,
            "input_path": str(png),
            "input_sha256": hashlib.sha256(raw).hexdigest(),
            "source_hwc_shape": list(image.shape),
            "detector_input": capture_one(operators, image),
        }
        records.append(record)
        print(
            f"{fixture}: {image.shape[1]}x{image.shape[0]} -> "
            f"{record['detector_input']['shape']}"
        )

    recognizer_batches = capture_recognizer_batches()

    document = {
        "capture_schema_version": CAPTURE_SCHEMA_VERSION,
        "stage": "detector_input_and_recognizer_input",
        "recognizer": {
            "crop_sizes": RECOGNIZER_CROP_SIZES,
            "recognition_shape": list(RECOGNITION_SHAPE),
            "batch_size": RECOGNITION_BATCH_SIZE,
            "batches": recognizer_batches,
        },
        "upstream": {
            "reference_paths": [
                "ppocr/data/imaug/operators.py",
                "tools/infer/predict_det.py",
                "tools/infer/utility.py",
            ],
            "operators": [
                {
                    "DetResizeForTest": {
                        "limit_side_len": DET_LIMIT_SIDE_LEN,
                        "limit_type": DET_LIMIT_TYPE,
                    }
                },
                {"NormalizeImage": NORMALIZE},
                {"ToCHWImage": None},
            ],
        },
        "records": records,
    }
    output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {output}")

    fixture = {
        "schema_version": FIXTURE_SCHEMA_VERSION,
        "stage": "detector_input_and_recognizer_input",
        "sample_count": SAMPLE_COUNT,
        "operators": document["upstream"]["operators"],
        "records": [compact_record(record) for record in records],
        "recognizer": {
            "crop_sizes": RECOGNIZER_CROP_SIZES,
            "batch_size": RECOGNITION_BATCH_SIZE,
            "batches": [compact_batch(batch) for batch in recognizer_batches],
        },
    }
    fixture_output.write_text(
        json.dumps(fixture, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"wrote {fixture_output} ({fixture_output.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
