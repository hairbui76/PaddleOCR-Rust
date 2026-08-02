# OpenCV Crop Oracle Capture

Roadmap items: `CROP-001`, `GEO-002`, `FIX-001`, `TOL-001`
Status: One reviewed component capture and its inverse-mapping sidecar are committed; no model-backed capture exists
Baseline: PaddleOCR commit `2661c7c0ef5c613e8f93c6e93b2e052399f0f854`

## Purpose and boundary

[`tools/capture_crop_oracle.py`](../tools/capture_crop_oracle.py) is a small,
developer-only OpenCV oracle for the private perspective-crop sequence in
`tools/infer/utility.py:get_rotate_crop_image` of the pinned upstream source.
It uses only self-authored in-memory BGR `uint8` arrays and source
quadrilaterals. It neither imports nor executes the `PaddleOCR/` checkout,
loads models, downloads assets, writes fixture files, or participates in a
Rust build, test, package, or runtime path.

The tool captures the required `cv2.getPerspectiveTransform` plus
`cv2.warpPerspective` configuration (`INTER_CUBIC`, `BORDER_REPLICATE`) and
the post-warp `numpy.rot90` condition. It records the actual Python, OpenCV,
NumPy, and installed OpenCV distribution versions, an OpenCV build-information
SHA-256, the calculated perspective matrix, and SHA-256/base64 representations
of the self-authored input and output bytes.

This is component evidence only. It does not establish decoder behavior,
OpenCV version parity with a particular upstream environment, detector
postprocessing, model inference, or OCR compatibility.

## Isolation and prerequisites

Run it only from an explicitly chosen working directory outside both this Rust
checkout and `PaddleOCR/`. Provision a Python environment outside both
repositories with a reviewed OpenCV and NumPy installation. The tool does not
install those packages and does not make a version choice for the Rust project.

The `--list` mode has no optional-package dependency and is useful for
discovering the fixed corpus:

```sh
python3 /path/to/PaddleOCR-Rust/tools/capture_crop_oracle.py --list
```

After a reviewer has provisioned a suitable isolated environment, capture all
cases by writing stdout to an explicit external path:

```sh
cd /path/outside/both-repositories
python3 /path/to/PaddleOCR-Rust/tools/capture_crop_oracle.py > crop-oracle.json
```

The separately consumable pre-rotation inverse-mapping oracle uses the same
fixed self-authored cases and environment, but writes a line-oriented CSV
record rather than crop bytes:

```sh
cd /path/outside/both-repositories
python3 /path/to/PaddleOCR-Rust/tools/capture_crop_oracle.py \
  --inverse-mapping-oracle > crop-inverse-mappings.csv
```

The input corpus currently covers identity bytes, left-border replication, a
small fractional projective transform, the exact `height / width == 1.5`
rotation boundary, a non-linear interior projective crop, a non-linear crop
crossing every image side, a non-linear tall projective crop before rotation,
eighth-pixel interior phases, a one-by-one fractional result, and a one-pixel
wide tall projective result. The corpus is BGR only because the frozen M2
classic input contract starts from a decoded OpenCV-style BGR image.
Decoder/color/alpha semantics remain separate `D-008` and `IMG-*` work.

## Reviewed capture

The reviewed capture is
[tests/fixtures/classic-v1-crop-oracle/capture.json](../tests/fixtures/classic-v1-crop-oracle/capture.json).
It was captured on 2026-08-02 with Python 3.12.3, NumPy 2.5.1, OpenCV 5.0.0,
and opencv-python-headless 5.0.0.93. Its exact JSON SHA-256 is
`dce8b7bae354c66a73fb8ec11045665eaee8c23cc3f7d960e710a9b3c9739a38`;
[metadata.json](../tests/fixtures/classic-v1-crop-oracle/metadata.json) records
the raw-byte aggregate hashes, upstream reference, review date, and limits.

The sidecar
[tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv](../tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv)
has SHA-256 `1b0fbde354120c6ce889e709e3fc324635fe8dbfcbb84caeb027cca7dfa4ae9b`.
It records fifty `warp → source` points: the four pre-rotation destination
boundaries and one interior coordinate for each of the ten reviewed cases.
The expected coordinates are independent OpenCV
`cv2.getPerspectiveTransform(destination, source)` plus
`cv2.perspectiveTransform` evaluations, not values calculated by Rust.

The offline Rust regressions
`crop::tests::classic_crop_matches_the_captured_opencv_bgr_oracle_cases` and
`crop::tests::classic_crop_matches_extended_opencv_projective_bgr_oracle_cases`,
and `crop::tests::classic_crop_matches_fractional_extent_opencv_oracle_cases`
check all ten recorded outputs without importing Python or OpenCV. Exact
agreement is evidence only for these self-authored BGR cases and this recorded
environment. It is not a claim of universal OpenCV interpolation parity,
upstream-environment parity, decoded-image behavior, or OCR compatibility.

`geometry::tests::classic_crop_plan_matches_recorded_opencv_interior_mappings`
also checks selected non-corner source-to-warp coordinates against
`cv2.perspectiveTransform` evaluations of the captured matrices for the phase,
one-pixel, and tall-thin cases. This is narrow mapping evidence for those
recorded matrices, not general OpenCV homography equivalence.

`geometry::tests::classic_crop_plan_matches_captured_opencv_inverse_mapping_oracle`
parses the sidecar offline and checks all fifty captured pre-rotation
warp-to-source coordinates against the private plan. It therefore covers the
mapping direction used by the crop sampler, while remaining limited to this
recorded OpenCV environment and the self-authored cases.

## Review and promotion procedure

1. Record the isolated environment, package provenance, and the command used.
   Do not treat a screen-visible output as evidence.
2. Review every JSON field, especially `environment`, points, perspective
   matrix, pre-rotation dimensions, rotation flag, and input/output SHA-256
   values. For an inverse sidecar, also review every source quadrilateral,
   destination coordinate, expected source coordinate, and sidecar SHA-256.
3. Preserve the reviewed JSON and any sidecar as candidate evidence artifacts
   outside the repository first. Compare them against the Rust private crop
   output and inverse mapping in deliberately written offline tests.
4. Only after source/license review, promote minimal expected-byte and mapping
   fixtures and complete fixture metadata under `tests/fixtures/`. The metadata
   must identify the exact OpenCV/NumPy capture versions and selected tolerance.
5. A mismatch opens a `CROP-001` or `GEO-002` investigation. Do not alter Rust interpolation,
   geometry, or expected bytes merely to make the test pass.

The full isolated model/oracle procedure remains
[`ORACLE_CAPTURE.md`](ORACLE_CAPTURE.md). That procedure is still mandatory
for model-backed/end-to-end M2 evidence.
