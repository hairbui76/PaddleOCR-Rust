#!/usr/bin/env python3
"""Capture upstream detector input tensors for the committed fixture inputs.

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

    document = {
        "capture_schema_version": CAPTURE_SCHEMA_VERSION,
        "stage": "detector_input",
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
        "stage": "detector_input",
        "sample_count": SAMPLE_COUNT,
        "operators": document["upstream"]["operators"],
        "records": [compact_record(record) for record in records],
    }
    fixture_output.write_text(
        json.dumps(fixture, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"wrote {fixture_output} ({fixture_output.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
