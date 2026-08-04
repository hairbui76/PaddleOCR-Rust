#!/usr/bin/env python3
"""Capture the document orientation classifier's input tensor and output.

Roadmap item: `DOCORI-001`, document-level half.

Its preprocessing is a shape this project has never implemented: resize the
**shorter** side to a target with the aspect ratio preserved, then take a
**centre crop**. Both steps are transcribed from the C++ deployment source rather
than guessed, because the artifact config names the operators and the source
defines what they do:

    ResizeByShort   scale = target / min(h, w)
                    h_resize = round(h * scale), w_resize = round(w * scale)
                    cv2.resize(..., INTER_LINEAR)
                    -- deploy/cpp_infer/src/common/processors.cc:154

    Crop "Center"   x1 = max(0, (w - crop_w) / 2)   integer division
                    y1 = max(0, (h - crop_h) / 2)
                    error if the image is smaller than the crop
                    -- deploy/cpp_infer/src/modules/image_classification/processors.cc:40

The rounding and the integer division are the details worth capturing: a
half-pixel difference in either moves the crop window by a whole pixel, and every
value in the tensor with it.

Needs `numpy`, `cv2`, and `onnxruntime`. Nothing is downloaded.

Usage:
    python3 tools/capture_document_orientation_oracle.py <model dir> <output.json>
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

import numpy as np

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/document-orientation-oracle-capture/v1"

# From PP-LCNet_x1_0_doc_ori/inference.yml.
RESIZE_SHORT = 256
CROP_SIZE = 224
SCALE = 1.0 / 255.0
MEAN = [0.485, 0.456, 0.406]
STD = [0.229, 0.224, 0.225]
LABELS = ["0", "90", "180", "270"]

# Deterministic synthetic pages, chosen so the resize hits landscape, portrait,
# square, and a case whose scaled dimension needs rounding rather than truncation.
SYNTHETIC_SIZES = [(400, 300), (300, 400), (256, 256), (513, 371)]


def synthetic_page(index: int, width: int, height: int) -> np.ndarray:
    x = np.arange(width, dtype=np.int64)[None, :, None]
    y = np.arange(height, dtype=np.int64)[:, None, None]
    c = np.arange(3, dtype=np.int64)[None, None, :]
    return ((x * 7 + y * 13 + c * 29 + index * 31) % 256).astype(np.uint8)


def resize_by_short(image: np.ndarray, target: int) -> np.ndarray:
    import cv2

    height, width = image.shape[:2]
    scale = target / float(min(height, width))
    # `static_cast<int>(std::round(...))` — round half away from zero, which for
    # positive values is what Python's round() does *not* do, so it is written
    # out rather than relying on the built-in's banker's rounding.
    resized_h = int(np.floor(height * scale + 0.5))
    resized_w = int(np.floor(width * scale + 0.5))
    return cv2.resize(image, (resized_w, resized_h), interpolation=cv2.INTER_LINEAR)


def centre_crop(image: np.ndarray, size: int) -> np.ndarray:
    height, width = image.shape[:2]
    if width < size or height < size:
        raise SystemExit(f"image {width}x{height} smaller than crop {size}")
    x1 = max(0, (width - size) // 2)
    y1 = max(0, (height - size) // 2)
    return image[y1 : y1 + size, x1 : x1 + size]


def preprocess(image: np.ndarray) -> np.ndarray:
    resized = resize_by_short(image, RESIZE_SHORT)
    cropped = centre_crop(resized, CROP_SIZE)
    normalized = cropped.astype("float32") * SCALE
    normalized = (normalized - np.array(MEAN, dtype="float32")) / np.array(
        STD, dtype="float32"
    )
    return np.ascontiguousarray(normalized.transpose((2, 0, 1)), dtype="<f4")


def encode(tensor: np.ndarray) -> dict:
    raw = np.ascontiguousarray(tensor, dtype="<f4").tobytes()
    return {
        "shape": list(tensor.shape),
        "values_sha256": hashlib.sha256(raw).hexdigest(),
        "values_base64": base64.b64encode(raw).decode("ascii"),
    }


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    model_dir = Path(sys.argv[1])
    output = Path(sys.argv[2])

    import cv2
    import onnxruntime

    model = model_dir / "inference.onnx"
    session = onnxruntime.InferenceSession(
        str(model), providers=["CPUExecutionProvider"]
    )
    input_name = session.get_inputs()[0].name
    output_name = session.get_outputs()[0].name

    cases = [
        (f"synthetic-{index}", synthetic_page(index, width, height))
        for index, (width, height) in enumerate(SYNTHETIC_SIZES)
    ]

    page = cv2.imdecode(
        np.frombuffer(
            Path("tests/fixtures/classic-v1-benchmark-page/input.png").read_bytes(),
            dtype=np.uint8,
        ),
        cv2.IMREAD_COLOR,
    )
    if page is None:
        raise SystemExit("could not decode the benchmark page")
    cases.append(("benchmark-page", page))
    for angle, code in [
        (90, cv2.ROTATE_90_CLOCKWISE),
        (180, cv2.ROTATE_180),
        (270, cv2.ROTATE_90_COUNTERCLOCKWISE),
    ]:
        cases.append((f"benchmark-page-{angle}", cv2.rotate(page, code)))

    records = []
    for name, image in cases:
        tensor = preprocess(image)
        batch = tensor[np.newaxis, :]
        row = np.asarray(
            session.run([output_name], {input_name: batch})[0][0], dtype="<f4"
        )
        winner = int(np.argmax(row))
        records.append(
            {
                "case": name,
                "source_hwc_shape": list(image.shape),
                "source_bgr_sha256": hashlib.sha256(
                    np.ascontiguousarray(image).tobytes()
                ).hexdigest(),
                "resized_hwc_shape": list(resize_by_short(image, RESIZE_SHORT).shape),
                "input": encode(batch),
                "output": encode(row),
                "label": LABELS[winner],
                "score": float(row[winner]),
            }
        )
        print(f"{name}: {LABELS[winner]} {float(row[winner]):.6f}")

    document = {
        "capture_schema_version": CAPTURE_SCHEMA_VERSION,
        "model": {
            "name": "PP-LCNet_x1_0_doc_ori",
            "sha256": hashlib.sha256(model.read_bytes()).hexdigest(),
            "input_name": input_name,
            "output_name": output_name,
        },
        "preprocess": {
            "resize_short": RESIZE_SHORT,
            "crop": CROP_SIZE,
            "scale": SCALE,
            "mean": MEAN,
            "std": STD,
            "labels": LABELS,
        },
        "records": records,
    }
    output.write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
