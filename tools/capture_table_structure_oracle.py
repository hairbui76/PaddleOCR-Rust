#!/usr/bin/env python3
"""Capture the table structure recognizer's input tensor and token decode.

Roadmap item: `TBLSTRUCT-001`, for `SLANeXt_wired` and its wireless twin.

This is the first `P8` module whose contract is not a variation on one already
ported. Its preprocessing chain is new in three ways and its postprocess is a
**token grammar** rather than a box decode:

    DecodeImage          BGR -- not RGB, unlike every other PaddleX model here
    ResizeTableImage     ResizeByLong(512), aspect preserved
    NormalizeImage       ImageNet
    PaddingTableImage    pad to 512x512 with 0 -- AFTER normalizing
    ToCHWImage
    TableLabelDecode     argmax over a 47-token vocabulary

Three details are in the registration functions rather than in the operators, and
reading either alone gives the wrong answer:

  * `build_normalize` **discards the config's `scale`**. The config says
    `scale: '1./255.'` -- a string -- and the registration never forwards it, so
    `Normalize`'s own default is what runs. A different scale in the config would
    be silently ignored.
  * `build_padding` passes `pad_value=0`, its own default. `Pad.__init__`
    defaults to `127.5`. Reading `Pad` alone gives a grey border where upstream
    writes zeros.
  * The pad runs **after** the normalize, so those zeros are zeros in normalized
    space -- not black pixels, which would be `-2.117` in the first channel.

Like the other PaddleX captures, this one **imports and executes** the pinned
operators. The token decode is captured against synthetic probabilities: it is
fully determined by the probability tensor, so real ones would pin nothing extra
and would require an inference session.

Needs `numpy`, `cv2`, and `paddlex` 3.7.2. Nothing is downloaded.

Usage:
    python3 tools/capture_table_structure_oracle.py <model dir> <output.json>
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
import yaml

from paddlex.inference.models.common.vision.processors import (
    Normalize,
    ResizeByLong,
    ToBatch,
    ToCHWImage,
)
from paddlex.inference.models.table_structure_recognition.processors import (
    Pad,
    TableLabelDecode,
)

CAPTURE_SCHEMA_VERSION = "paddleocr-rust/table-structure-oracle-capture/v1"

LONG_EDGE = 512
PAD_SIDE = 512
PAD_VALUE = 0
MEAN = [0.485, 0.456, 0.406]
STD = [0.229, 0.224, 0.225]

SAMPLE_COUNT = 256


def synthetic_bgr(width: int, height: int) -> np.ndarray:
    y = np.arange(height, dtype=np.int64)[:, None, None]
    x = np.arange(width, dtype=np.int64)[None, :, None]
    c = np.arange(3, dtype=np.int64)[None, None, :]
    return ((x * 7 + y * 13 + c * 29) % 256).astype(np.uint8)


def sample_indices(count: int, total: int) -> list[int]:
    if total <= count:
        return list(range(total))
    step = total / count
    return [min(total - 1, int(i * step)) for i in range(count)]


def capture_tensor(name: str, image_bgr: np.ndarray) -> dict:
    resize = ResizeByLong(target_long_edge=LONG_EDGE)
    normalize = Normalize(mean=MEAN, std=STD)
    pad = Pad(target_size=PAD_SIDE, val=PAD_VALUE)
    to_chw = ToCHWImage()
    to_batch = ToBatch()

    height, width = image_bgr.shape[:2]
    resized = resize(imgs=[image_bgr])
    normalized = normalize(imgs=resized)
    padded = pad(imgs=normalized)
    pad_img, padding_size = padded[0]
    chw = to_chw(imgs=[pad_img])
    values = np.ascontiguousarray(to_batch(imgs=chw)[0], dtype=np.float32)

    flat = values.reshape(-1)
    indices = sample_indices(SAMPLE_COUNT, flat.size)
    samples = np.asarray([flat[i] for i in indices], dtype=np.float32)

    return {
        "case": name,
        "source_wh": [int(width), int(height)],
        "source_bgr_sha256": hashlib.sha256(
            np.ascontiguousarray(image_bgr).tobytes()
        ).hexdigest(),
        "resized_hwc_shape": list(resized[0].shape),
        "padding_size_wh": [int(v) for v in padding_size],
        "input_shape": list(values.shape),
        "input_values_sha256": hashlib.sha256(flat.tobytes()).hexdigest(),
        "input_sample_indices": indices,
        "input_sample_values_base64": base64.b64encode(samples.tobytes()).decode("ascii"),
    }


def capture_decode(
    name: str,
    model_name: str,
    dict_character: list[str],
    token_ids: list[int],
    ori_wh: list[int],
) -> dict:
    """One decode, driven by a probability tensor built from a token sequence.

    `dict_character` is copied per call because `TableLabelDecode.__init__`
    **mutates the list it is given** — it removes `<td>` and appends `<td></td>`
    in place. A caller that reuses the config's own list gets a different
    vocabulary the second time.
    """
    decoder = TableLabelDecode(
        model_name=model_name,
        merge_no_span_structure=True,
        dict_character=list(dict_character),
    )
    vocabulary = len(decoder.character)

    # A one-hot-ish tensor: the chosen token at 0.9, the rest sharing the mass.
    # Distinct per position so `argmax` and `max` both have a definite answer.
    probs = np.full((1, len(token_ids), vocabulary), 0.1 / (vocabulary - 1), dtype=np.float32)
    for position, token in enumerate(token_ids):
        probs[0, position, token] = 0.9

    # Eight coordinates per position, in the `xyxyxyxy` order the config declares.
    boxes = np.zeros((1, len(token_ids), 8), dtype=np.float32)
    for position in range(len(token_ids)):
        base = 0.05 + 0.01 * position
        boxes[0, position] = [
            base, base, base + 0.2, base,
            base + 0.2, base + 0.1, base, base + 0.1,
        ]

    results = decoder(
        pred=[boxes, probs],
        img_size=[[PAD_SIDE, PAD_SIDE]],
        ori_img_size=[ori_wh],
    )
    result = results[0]
    return {
        "case": name,
        "model_name": model_name,
        "token_ids": token_ids,
        "ori_wh": ori_wh,
        "vocabulary_size": vocabulary,
        "structure": list(result["structure"]),
        "structure_score": float(result["structure_score"]),
        "bbox": [[int(v) for v in row] for row in result["bbox"]],
    }


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    model_dir, output = Path(sys.argv[1]), Path(sys.argv[2])
    config_path = model_dir / "inference.yml"
    onnx = model_dir / "inference.onnx"
    if not config_path.is_file():
        print(f"missing: {config_path}", file=sys.stderr)
        return 2
    config = yaml.safe_load(config_path.read_text())
    dict_character = list(config["PostProcess"]["character_dict"])

    records = [
        # Landscape, so the long edge is the width.
        capture_tensor("landscape_800x300", synthetic_bgr(800, 300)),
        # Portrait, so the long edge is the height and the padding is on the right.
        capture_tensor("portrait_300x800", synthetic_bgr(300, 800)),
        # Already square at the target: no resize, no padding.
        capture_tensor("square_512", synthetic_bgr(512, 512)),
        # Upscaled, and 341.33 rounds to 341 with nothing on a tie.
        capture_tensor("small_256x171", synthetic_bgr(256, 171)),
        # 1025 * (512/1025) is exactly 512 on the long edge, and the short edge
        # lands on 256.5 -- the round-half-to-even boundary again.
        capture_tensor("half_boundary_513x1025", synthetic_bgr(513, 1025)),
    ]

    # Token ids are resolved against the *merged* vocabulary, which is what the
    # decoder builds: `<td>` removed, `<td></td>` appended, `sos`/`eos` wrapped.
    probe = TableLabelDecode(
        model_name="SLANeXt_wired",
        merge_no_span_structure=True,
        dict_character=list(dict_character),
    )
    index = {token: position for position, token in enumerate(probe.character)}

    simple = [
        index["<thead>"], index["<tr>"], index["<td></td>"], index["</tr>"],
        index["</thead>"], index["<tbody>"], index["<tr>"], index["<td></td>"],
        index["</tr>"], index["</tbody>"], index["eos"],
    ]
    spanning = [
        index["<tbody>"], index["<tr>"], index["<td"], index[' colspan="2"'],
        index[">"], index["</td>"], index["</tr>"], index["</tbody>"], index["eos"],
    ]
    # `sos` first, which the decoder must ignore rather than emit.
    leading_sos = [index["sos"], index["<tbody>"], index["<tr>"], index["<td></td>"],
                   index["</tr>"], index["</tbody>"], index["eos"]]
    # No `eos` at all: the decoder must run to the end of the sequence.
    unterminated = [index["<tbody>"], index["<tr>"], index["<td></td>"], index["</tr>"]]

    decodes = [
        capture_decode("simple_table", "SLANeXt_wired", dict_character, simple, [640, 480]),
        capture_decode("spanning_cell", "SLANeXt_wired", dict_character, spanning, [640, 480]),
        capture_decode("leading_sos", "SLANeXt_wired", dict_character, leading_sos, [640, 480]),
        capture_decode("unterminated", "SLANeXt_wired", dict_character, unterminated, [640, 480]),
        # SLANet takes the other branch of `_get_bbox_scales`, which uses the
        # original size directly rather than the padded size over the ratio.
        capture_decode("slanet_scaling", "SLANet", dict_character, simple, [640, 480]),
    ]

    document = {
        "schema_version": CAPTURE_SCHEMA_VERSION,
        "model": {
            "name": config["Global"]["model_name"],
            "sha256": hashlib.sha256(onnx.read_bytes()).hexdigest() if onnx.is_file() else None,
            "config_sha256": hashlib.sha256(config_path.read_bytes()).hexdigest(),
        },
        "preprocess": {
            "long_edge": LONG_EDGE,
            "pad_side": PAD_SIDE,
            "pad_value": PAD_VALUE,
            "mean": MEAN,
            "std": STD,
            "channel_order": "BGR",
            "config_scale_is_ignored": config["PreProcess"]["transform_ops"][4][
                "NormalizeImage"
            ]["scale"],
        },
        "vocabulary": list(probe.character),
        "sample_count": SAMPLE_COUNT,
        "records": records,
        "decodes": decodes,
    }
    output.write_text(json.dumps(document, indent=1, sort_keys=True) + "\n")
    print(f"wrote {output} ({len(records)} tensors, {len(decodes)} decodes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
