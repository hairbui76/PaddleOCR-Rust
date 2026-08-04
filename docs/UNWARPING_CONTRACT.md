# Document Unwarping Contract

Roadmap item: `UNWARP-001` (contract half)
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`
Status: contract frozen and artifact provisioned; **no implementation**

Frozen before any Rust is written, as `ORIENTATION_CONTRACT.md` was. That order
paid for itself twice on orientation — the contract was wrong in two ways that
would each have produced working code against the wrong model — so it is the
order used again here.

## 1. What upstream actually does

`deploy/cpp_infer/src/configs/OCR.yaml` pins model `UVDoc` under
`DocUnwarping`, inside the `doc_preprocessor` sub-pipeline. The implementation
is `deploy/cpp_infer/src/modules/image_unwarping/`. There is **no Python
predictor**, the same situation as document orientation.

`WarpPredictor::Build` registers exactly four operations:

```
ReadImage   BGR
Normalize   scale 1/255, mean 0, std 1
ToCHWImage
ToBatch
```

and one postprocess, `DocTr`, which multiplies the output by `255` and converts
to `uint8`.

## 2. Three things that make this unlike every capability so far

### There is no resize

Not in `Build`, not in the pipeline that calls it. The image is normalized and
handed to the model at its native size. Confirmed against the provisioned
artifact: it accepts `1x3x256x128`, `1x3x128x64`, and `1x3x300x400` and returns
the **same shape** each time. The declared TensorRT shapes name `256x128` as the
optimum, not a requirement.

This is the first model in this project with no fixed or derived input shape.
Every resource bound that has protected the pipeline so far — a fixed `48x320`,
a computed `960` limit side, a fixed `160x80` — is absent, so a caller's page
size flows straight into the tensor.

### The normalization is a third convention

| Path | Normalization |
|---|---|
| Detector, both orientation classifiers | `(x/255 − mean) / std`, ImageNet constants |
| Recognizer, legacy text-line classifier | `(x/255 − 0.5) / 0.5` |
| **UVDoc** | **`x/255`, mean `0`, std `1`** |

Three conventions across five models in one pipeline. Reusing the wrong one
produces a plausible image rather than an error.

### The output is an image, not a transform

`DocTr` returns the unwarped pixels. The model does not emit a displacement
field, a homography, or control points — nothing that can be inverted.

## 3. The consequence that matters: unwarping is not invertible

Every geometry-changing stage so far has had an inverse. The detector resize has
a recorded ratio; the perspective crop has `ImageTransform::inverse`; the page
rotation has `DocumentRotation::inverse`, whose whole purpose is returning
coordinates to the caller's image.

Unwarping has none. A curved page is resampled into a flat one by a learned,
per-pixel deformation that this project never sees. Text detected on the
unwarped image has coordinates **in the unwarped image**, and there is no way to
map them back to the photograph the caller supplied.

That is not a limitation of this port. It is what upstream does: the
`doc_preprocessor` pipeline returns the unwarped image and everything downstream
operates on it.

**So `UNWARP-001` cannot promise what `DOCORI-001` promised.** A caller who
enables unwarping gets polygons describing an image they did not supply, and any
implementation must say so plainly rather than returning coordinates that look
like they belong to the input. That is a public API question, and it belongs in
`DOCPIPE-001` alongside the decision of what a document-preprocessing result
even contains.

## 4. The provisioned artifact

Apache-2.0, stored outside version control per `MODEL-DEC-001`.

| Field | Value |
|---|---|
| Model | `UVDoc`, from [`PaddlePaddle/UVDoc_onnx`](https://huggingface.co/PaddlePaddle/UVDoc_onnx) |
| Revision | `3bcf535371727d11e783101f79a504c68848aae3` |
| `inference.onnx` | SHA-256 `54cab30dc2cf347b4f3d6971c833b1c3d84f5ef17280f1f6d15796bff20e63d6`, `31,684,150` bytes |
| `inference.yml` | SHA-256 `be83d537b358f3ff87740e77e14a83ee9e9a7bb215c33d091b69e8bd5904fe39`, `330` bytes |
| Input tensor | `image`, `[N, 3, H, W]`, all of `N`, `H`, `W` dynamic |
| Output tensor | `fetch_name_0`, `[N, 3, H, W]`, matching the input |

At `31.7 MB` it is five times either orientation classifier and half the
detector.

Note that its `inference.yml` declares **no** `PreProcess` or `PostProcess`
block at all — only TensorRT shape hints. Unlike the orientation models, the
contract here is entirely in the C++ source, so provisioning the artifact was not
a prerequisite for freezing it. It was still worth doing to confirm the tensor
signature, which is how the "no resize" claim above became a measurement rather
than an inference from an absent registration.

## 5. What an implementation must produce

1. **A resource bound.** The first thing to decide, because there is no natural
   one: an unbounded input flows straight into a `H x W x 3 x 4`-byte tensor and
   a same-sized output. The existing `40,000,000`-pixel limit gives `960 MB` in
   float tensors alone, which the `2 GiB` budget cannot absorb alongside two
   model sessions.
2. **A captured oracle**, in the shape `PRE-001` established: the input tensor
   compared elementwise, and the output image compared against a capture.
3. **A gate against the real artifact**, the `LANG-001` bar.
4. **An explicit position on coordinates**, per §3, before any public surface
   exposes it.
5. **A default of off**, matching `use_doc_unwarping` in the pipeline config.

## 6. A fourth rounding, found by the capture

`DocTr` ends with `convertTo(CV_8U)`, which goes through
`saturate_cast<uchar>` and therefore `cvRound` — **half to even**, not half away
from zero.

This was found by comparison, not by reading: the first implementation used
Rust's `f32::round`, which rounds half away from zero, and the captured image did
not match. It is the fourth rounding convention this project has had to pin,
after the recognizer's `ceil`, the batch width's truncation, and the page
rotation's truncated output size — and it is the same rule the detector's rescale
already needed, so the helper is now shared rather than written twice.

## 7. Oracle results

Three synthetic pages at `64x128`, `96x72`, and `33x17`. Every input tensor is
reproduced **bit-identically**, and the `uint8` conversion of every recorded raw
output matches the captured image. The two halves are checked separately, so a
failure in the normalization does not implicate the conversion.

The output shape equals the input shape in all three cases, which is what turns
"there is no resize" from an absent registration into a measurement.

## 8. Status

Contract frozen, artifact provisioned and hashed, and `src/unwarp.rs` implements
it: the bound first, then the normalization, the tensor layout, and the `uint8`
conversion — each matched against a capture.

The module is deliberately unreachable, for the reason §3 gives. Exposing
unwarping means returning polygons in an image the caller never supplied, with no
inverse available to fix that, and wiring it in before deciding what a caller is
told would ship coordinates that look like they belong to the input.

`UNWARP-001` therefore stays open on one question, and it is a public-API
question rather than an implementation one: what a document-preprocessing result
contains, and how it says which image its coordinates describe. That is
`DOCPIPE-001`.
