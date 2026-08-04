#!/usr/bin/env python3
"""Capture the table classifier's input tensor and its `Topk` postprocess.

Roadmap item: `TBLCLS-001`.

The first capability frozen by **executing** the pinned reference rather than
transcribing it. Every earlier capture reimplemented the upstream operators in
this file and relied on the reimplementation being faithful; this one imports
`paddlex` `3.7.2` and calls the operators themselves, so a transcription error
is not among the ways it can be wrong.

    ResizeByShort   scale = 256 / min(h, w)
                    h_resize = round(h * scale)     <- Python's round
                    cv2.resize(..., INTER_LINEAR)
    Crop "C"        x1 = max(0, (w - 224) // 2)
    Normalize       alpha = scale/std, beta = -mean/std, per channel
    ToCHWImage      transpose (2, 0, 1)
    Topk            argsort ascending, take the last k, reverse

Two of those disagree with the operators this project already has, and both
disagreements are silent:

  * `round` here is **Python's**, which rounds half to even. The C++
    `ResizeByShort` this project froze for document orientation uses
    `std::round`, which rounds half away from zero. Two of the cases below sit
    exactly on that boundary.
  * `Normalize` computes `x * (scale/std) + (-mean/std)`, not
    `(x * scale - mean) / std`. The same numbers in a different order, which in
    `f32` is not the same result.

Needs `numpy`, `cv2`, and `paddlex` 3.7.2. Nothing is downloaded, and no model
is run: the tensor and the postprocess are the whole compatibility surface here,
and both are reachable without inference.

Usage:
    python3 tools/capture_table_classification_oracle.py <model dir> <output.json>
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.models.common.vision.processors import (
    Normalize,
    ResizeByShort,
    ToBatch,
    ToCHWImage,
)
from paddlex.inference.models.image_classification.processors import Crop, Topk

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/table-classification-oracle-capture/v1"

# From PP-LCNet_x1_0_table_cls/inference.yml.
RESIZE_SHORT = 256
CROP_SIZE = 224
SCALE = 0.00392156862745098
MEAN = [0.485, 0.456, 0.406]
STD = [0.229, 0.224, 0.225]
LABELS = ["wired_table", "wireless_table"]
TOPK = 5

SAMPLE_COUNT = 256


def synthetic_rgb(width: int, height: int) -> np.ndarray:
    """A deterministic image, reproduced by the same formula in the Rust test.

    Committing pixels is avoided where a closed form will do, per the fixture
    policy: two of these are a megapixel each and exist only to land on a
    rounding boundary.
    """
    y = np.arange(height, dtype=np.int64)[:, None, None]
    x = np.arange(width, dtype=np.int64)[None, :, None]
    c = np.arange(3, dtype=np.int64)[None, None, :]
    return ((x * 7 + y * 13 + c * 29) % 256).astype(np.uint8)


def sample_indices(count: int, total: int) -> list[int]:
    """Evenly spaced positions, so the samples span the tensor rather than its
    first rows."""
    if total <= count:
        return list(range(total))
    step = total / count
    return [min(total - 1, int(i * step)) for i in range(count)]


def capture_tensor(name: str, image: np.ndarray) -> dict:
    resize = ResizeByShort(target_short_edge=RESIZE_SHORT, interp="LINEAR")
    crop = Crop(crop_size=CROP_SIZE)
    normalize = Normalize(scale=SCALE, mean=MEAN, std=STD)
    to_chw = ToCHWImage()
    to_batch = ToBatch()

    resized = resize(imgs=[image])
    cropped = crop(imgs=resized)
    normalized = normalize(imgs=cropped)
    chw = to_chw(imgs=normalized)
    batched = to_batch(imgs=chw)[0]

    values = np.ascontiguousarray(batched, dtype=np.float32)
    flat = values.reshape(-1)
    indices = sample_indices(SAMPLE_COUNT, flat.size)
    samples = np.asarray([flat[i] for i in indices], dtype=np.float32)

    return {
        "case": name,
        "source_hwc_shape": list(image.shape),
        "source_rgb_sha256": hashlib.sha256(
            np.ascontiguousarray(image).tobytes()
        ).hexdigest(),
        "resized_hwc_shape": list(resized[0].shape),
        "input_shape": list(values.shape),
        "input_values_sha256": hashlib.sha256(flat.tobytes()).hexdigest(),
        "input_sample_indices": indices,
        "input_sample_values_base64": base64.b64encode(samples.tobytes()).decode(
            "ascii"
        ),
    }


def capture_topk(name: str, logits: list[float]) -> dict:
    """`Topk` on a synthetic score vector.

    The model is not run. `Topk` consumes whatever the model emitted and its
    behaviour is fully determined by that vector, so synthetic scores pin it
    exactly as well as real ones and do not require an inference session.
    """
    topk = Topk(class_ids=LABELS)
    preds = [np.asarray([logits], dtype=np.float32)]
    indexes, scores, label_names = topk(preds, topk=TOPK)
    return {
        "case": name,
        "logits": [float(v) for v in logits],
        "indexes": [int(v) for v in indexes[0]],
        "scores": [float(v) for v in scores[0]],
        "labels": list(label_names[0]),
    }


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    model_dir = Path(sys.argv[1])
    output = Path(sys.argv[2])

    onnx = model_dir / "inference.onnx"
    config = model_dir / "inference.yml"
    for path in (onnx, config):
        if not path.is_file():
            print(f"missing: {path}", file=sys.stderr)
            return 2

    records = []

    # A plain portrait page, no boundary involved.
    records.append(capture_tensor("plain_portrait_297x421", synthetic_rgb(297, 421)))

    # 512 * (256/512) leaves the height at exactly 512.5. Python rounds it to
    # 512; `std::round` would give 513, and every value in the tensor after it
    # would differ.
    records.append(capture_tensor("half_rounds_down_512x1025", synthetic_rgb(512, 1025)))

    # 1030 * 0.25 is exactly 257.5, and 258 is the even neighbour, so this one
    # rounds *up* under the same rule. Both directions are needed: a capture
    # that only ever rounded down would also pass with truncation.
    records.append(capture_tensor("half_rounds_up_1024x1030", synthetic_rgb(1024, 1030)))

    # Short side already 256, so `ResizeByShort` scales by exactly 1 and
    # `F.resize` short-circuits without calling cv2 at all.
    records.append(capture_tensor("short_side_already_256", synthetic_rgb(256, 300)))

    # No resize and a crop from every side.
    records.append(capture_tensor("square_256", synthetic_rgb(256, 256)))

    # A wide page, so the crop window is taken from the horizontal centre.
    records.append(capture_tensor("wide_640x300", synthetic_rgb(640, 300)))

    postprocess = [
        capture_topk("wired_wins", [0.87, 0.13]),
        capture_topk("wireless_wins", [0.13, 0.87]),
        # `np.around(..., 5)` is half-to-even at the fifth decimal, and these
        # two sit on it.
        capture_topk("rounds_at_five_decimals", [0.123455, 0.876545]),
        # Equal scores: `argsort` decides, and which class wins is worth
        # pinning rather than leaving to chance.
        capture_topk("tied", [0.5, 0.5]),
    ]

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "model": {
            "name": "PP-LCNet_x1_0_table_cls",
            "sha256": hashlib.sha256(onnx.read_bytes()).hexdigest(),
            "config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(),
        },
        "preprocess": {
            "resize_short": RESIZE_SHORT,
            "crop": CROP_SIZE,
            "scale": SCALE,
            "mean": MEAN,
            "std": STD,
            "channel_order": "RGB",
            "labels": LABELS,
            "topk": TOPK,
        },
        "sample_count": SAMPLE_COUNT,
        "records": records,
        "postprocess": postprocess,
    }

    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} tensors, {len(postprocess)} topk cases)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
