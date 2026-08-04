#!/usr/bin/env python3
"""Capture the table cell detector's input tensor and batch inputs.

Roadmap item: `TBLCELL-001`.

`RT-DETR-L_wired_table_cell_det` and its wireless twin declare **the same
operator chain as the layout detector**, differing only in the target size and
the class list:

    Resize          target_size [640, 640], keep_ratio false, interp 2 (BICUBIC)
    NormalizeImage  norm_type "none" -- which PaddleX rewrites to mean_std, so
                    the effective transform is x/255
    Permute         HWC to CHW
    ToBatch         img_size, img, scale_factors -- reversed on the way in

`arch: DETR` puts both models in `models_required_imgsize`, so they take the same
three inputs the layout detector takes. That makes this a **parameterization** of
what `LAY-001` already froze, not a new contract, and the capture exists to prove
that claim rather than to assume it.

Like the table classifier's capture, this one **imports and executes** the pinned
PaddleX operators rather than transcribing them.

Needs `numpy`, `cv2`, and `paddlex` 3.7.2. Nothing is downloaded and no model is
run: the tensor and the batch inputs are the whole preprocessing surface.

Usage:
    python3 tools/capture_table_cells_oracle.py <output.json>
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

import numpy as np

from paddlex.inference.models.object_detection.processors import (
    Normalize,
    Resize,
    ToBatch,
    ToCHWImage,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/table-cells-oracle-capture/v1"

# From RT-DETR-L_wired_table_cell_det/inference.yml, which is byte-identical to
# the wireless model's on every field this capture reads.
INPUT_SIDE = 640
THRESHOLD = 0.5
LABELS = ["cell"]
SCALE = 1.0 / 255.0

SAMPLE_COUNT = 256


def synthetic_rgb(width: int, height: int) -> np.ndarray:
    """The same closed form the table classifier's capture uses."""
    y = np.arange(height, dtype=np.int64)[:, None, None]
    x = np.arange(width, dtype=np.int64)[None, :, None]
    c = np.arange(3, dtype=np.int64)[None, None, :]
    return ((x * 7 + y * 13 + c * 29) % 256).astype(np.uint8)


def sample_indices(count: int, total: int) -> list[int]:
    if total <= count:
        return list(range(total))
    step = total / count
    return [min(total - 1, int(i * step)) for i in range(count)]


def capture_tensor(name: str, image: np.ndarray) -> dict:
    # `target_size[::-1]` happens inside `build_resize`; 640x640 is symmetric, so
    # it is reproduced here explicitly rather than relied upon.
    resize = Resize(target_size=[INPUT_SIDE, INPUT_SIDE][::-1], keep_ratio=False, interp="BICUBIC")
    normalize = Normalize(scale=SCALE, mean=[0.0, 0.0, 0.0], std=[1.0, 1.0, 1.0])
    to_chw = ToCHWImage()
    to_batch = ToBatch(ordered_required_keys=("img_size", "img", "scale_factors"))

    datas = [{"img": image}]
    datas = resize(datas)
    resized_hwc = list(datas[0]["img"].shape)
    scale_factors_forward = list(datas[0]["scale_factors"])
    datas = normalize(datas)
    datas = to_chw(datas)
    batch = to_batch(datas)

    img_size, img, scale_factors = batch
    values = np.ascontiguousarray(img, dtype=np.float32)
    flat = values.reshape(-1)
    indices = sample_indices(SAMPLE_COUNT, flat.size)
    samples = np.asarray([flat[i] for i in indices], dtype=np.float32)

    return {
        "case": name,
        "source_hwc_shape": list(image.shape),
        "source_rgb_sha256": hashlib.sha256(
            np.ascontiguousarray(image).tobytes()
        ).hexdigest(),
        "resized_hwc_shape": resized_hwc,
        # What `Resize` computed, [w_scale, h_scale] ...
        "scale_factors_forward": [float(v) for v in scale_factors_forward],
        # ... and what `ToBatch` actually hands the model, reversed.
        "scale_factors_batched": [float(v) for v in np.asarray(scale_factors)[0]],
        # `im_shape`, likewise reversed by `ToBatch` into [h, w].
        "img_size_batched": [float(v) for v in np.asarray(img_size)[0]],
        "input_shape": list(values.shape),
        "input_values_sha256": hashlib.sha256(flat.tobytes()).hexdigest(),
        "input_sample_indices": indices,
        "input_sample_values_base64": base64.b64encode(samples.tobytes()).decode(
            "ascii"
        ),
    }


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    output = Path(sys.argv[1])

    records = [
        # A table crop of ordinary proportions.
        capture_tensor("table_crop_297x421", synthetic_rgb(297, 421)),
        # Wider than tall, so the two scale factors differ noticeably and a
        # transposed pair would be visible rather than plausible.
        capture_tensor("wide_960x240", synthetic_rgb(960, 240)),
        # Already square at the target: `keep_ratio: false` means this is the
        # one case where no scaling happens on either axis.
        capture_tensor("square_640", synthetic_rgb(640, 640)),
        # Upscaled on both axes.
        capture_tensor("small_120x90", synthetic_rgb(120, 90)),
    ]

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "model": {
            "names": [
                "RT-DETR-L_wired_table_cell_det",
                "RT-DETR-L_wireless_table_cell_det",
            ],
            "arch": "DETR",
        },
        "preprocess": {
            "input_side": INPUT_SIDE,
            "threshold": THRESHOLD,
            "labels": LABELS,
            "scale": SCALE,
            "channel_order": "RGB",
            "batch_input_order": ["img_size", "img", "scale_factors"],
        },
        "sample_count": SAMPLE_COUNT,
        "records": records,
    }

    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} tensors)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
