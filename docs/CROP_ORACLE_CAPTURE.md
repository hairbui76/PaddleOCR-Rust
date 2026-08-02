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
wide tall projective result, a high-variation case that crosses a cubic
half-byte rounding boundary before tall-result rotation, a high-variation
tall crop that detects the `f32` cubic-weight construction order, a
high-variation crop that detects the source-to-warp matrix inversion and
`f32` sampler-coordinate boundary, and a high-variation crop that detects
`getPerspectiveTransform` float32 coefficient construction and default LU
solving. The corpus is BGR only because the frozen M2
classic input contract starts from a decoded OpenCV-style BGR image.
Decoder/color/alpha semantics remain separate `D-008` and `IMG-*` work.

The cubic-weight regression follows the operation structure of OpenCV 5.0.0's
`bicubicWeights` source implementation rather than treating an algebraically
equivalent cubic polynomial as numerically interchangeable. It is a narrow
source-level explanation for the recorded fixture, not copied OpenCV code or a
general cross-version pixel-equivalence claim.

The sampling-matrix regression additionally follows OpenCV 5.0.0's
`warpPerspective` sequence: it inverts the supplied source-to-warp matrix and
the selected generic warp path converts the inverse to `Matx33f`, computes row
terms in `f32`, then calculates individual perspective coordinates through a
`double` division and casts them to `float`. The Rust implementation keeps this
as a private sampler detail; it is not a public geometry contract or a claim
about every OpenCV code path or platform.

The perspective-LU regression follows the selected OpenCV 5.0.0
`getPerspectiveTransform` path: its `Point2f` coefficient products are formed
in `float`, the eight equations are laid out as all horizontal rows followed
by all vertical rows, and the default `DECOMP_LU` routine solves that finite
precision system. The matching private 3-by-3 inverse uses the analytic
`CV_64F` inversion path used before sampling. These are private numerical
details established only for the recorded case, not a general claim that every
OpenCV matrix or platform has bit-identical behavior.

- OpenCV 5.0.0 `bicubicWeights` source: https://github.com/opencv/opencv/blob/5.0.0/modules/imgproc/src/warp_kernels.simd.hpp#L7000-L7010
- OpenCV 5.0.0 cubic horizontal/vertical accumulation: https://github.com/opencv/opencv/blob/5.0.0/modules/imgproc/src/warp_kernels.simd.hpp#L7160-L7319
- OpenCV 5.0.0 `warpPerspective` inversion: https://github.com/opencv/opencv/blob/5.0.0/modules/imgproc/src/imgwarp.cpp#L3013-L3059
- OpenCV 5.0.0 generic-warp `Matx33f` conversion and coordinate evaluation: https://github.com/opencv/opencv/blob/5.0.0/modules/imgproc/src/imgwarp.cpp#L88-L174
- OpenCV 5.0.0 `getPerspectiveTransform` equation construction: https://github.com/opencv/opencv/blob/5.0.0/modules/geometry/src/geometry.cpp#L769-L798
- OpenCV 5.0.0 default LU implementation: https://github.com/opencv/opencv/blob/5.0.0/modules/core/src/matrix_decomp.cpp#L14-L69
- OpenCV 5.0.0 analytic 3-by-3 inversion: https://github.com/opencv/opencv/blob/5.0.0/modules/core/src/lapack.cpp#L946-L967

## Reviewed capture

The reviewed capture is
[tests/fixtures/classic-v1-crop-oracle/capture.json](../tests/fixtures/classic-v1-crop-oracle/capture.json).
It was captured on 2026-08-02 with Python 3.12.3, NumPy 2.5.1, OpenCV 5.0.0,
and opencv-python-headless 5.0.0.93. Its exact JSON SHA-256 is
`7dcb1acbf1cb7a1e70c1a211f4583f11c11e23af0c2bad12b21a7641d92e7751`;
[metadata.json](../tests/fixtures/classic-v1-crop-oracle/metadata.json) records
the raw-byte aggregate hashes, upstream reference, review date, and limits.

The sidecar
[tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv](../tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv)
has SHA-256 `6fec6e7dd72f392d0b0ec100294649f0d8f1ade51c416cdbbcd75bc893d7b5a9`.
It records seventy `warp → source` points: the four pre-rotation destination
boundaries and one interior coordinate for each of the fourteen reviewed cases.
The expected coordinates are independent OpenCV
`cv2.getPerspectiveTransform(destination, source)` plus
`cv2.perspectiveTransform` evaluations, not values calculated by Rust.

The offline Rust regressions
`crop::tests::classic_crop_matches_the_captured_opencv_bgr_oracle_cases` and
`crop::tests::classic_crop_matches_extended_opencv_projective_bgr_oracle_cases`,
`crop::tests::classic_crop_matches_fractional_extent_opencv_oracle_cases`, and
`crop::tests::classic_crop_matches_cubic_rounding_opencv_oracle_case`, and
`crop::tests::classic_crop_matches_cubic_weight_construction_opencv_oracle_case`,
`crop::tests::classic_crop_matches_sampling_matrix_opencv_oracle_case`, and
`crop::tests::classic_crop_matches_perspective_lu_opencv_oracle_case`
check all fourteen recorded outputs without importing Python or OpenCV. Exact
agreement is evidence only for these self-authored BGR cases and this recorded
environment. It is not a claim of universal OpenCV interpolation parity,
upstream-environment parity, decoded-image behavior, or OCR compatibility.

`geometry::tests::classic_crop_plan_matches_recorded_opencv_interior_mappings`
also checks selected non-corner source-to-warp coordinates against
`cv2.perspectiveTransform` evaluations of the captured matrices for the phase,
one-pixel, and tall-thin cases. This is narrow mapping evidence for those
recorded matrices, not general OpenCV homography equivalence.

`geometry::tests::classic_crop_plan_matches_captured_opencv_inverse_mapping_oracle`
parses the sidecar offline and checks all seventy captured pre-rotation
warp-to-source coordinates against the private plan. It therefore covers the
mapping direction used by the crop sampler, while remaining limited to this
recorded OpenCV environment and the self-authored cases.

## Non-promoted optimization diagnostic

On 2026-08-02, an isolated, developer-only deterministic corpus of 4,096
self-authored BGR crops (3–16 pixel source sides, fixed seed, and strictly
convex quadrilaterals) was compared against the recorded OpenCV environment.
The current private Rust sampler differed from default `cv2.warpPerspective`
on fifteen cases, each by one `uint8` byte. The same corpus produced seven
one-byte differences between OpenCV's default path and
`cv2.setUseOptimized(False)`, demonstrating that CPU-optimized interpolation
is an independent numerical variable in that environment.

Read-only OpenCV source inspection and a focused boundary probe established
that fused multiply-add can change a value such as `89.499992` to exactly
`89.500000` before byte conversion. A temporary Rust `f32::mul_add`
experiment reduced the corpus count to thirteen mismatching cases but
introduced five new mismatches; a weight-only variant produced sixteen. No
FMA/SIMD implementation, fixture, tolerance change, or compatibility claim
was promoted. The selected scalar operation order remains the checked Rust
behavior, and the fourteen reviewed fixtures remain the only exact pixel
evidence.

Any future CPU-specific optimization work must first define the supported CPU
feature policy and test a portable operation-order contract across its target
matrix. It must not use a local OpenCV dispatch result as a universal pixel
oracle.

## Portable crop operation profile

The selected profile implements the `D-003` x86-64 baseline for the current
private crop sampler. The `quality` GitHub Actions job compiles the workspace
with:

```text
-C target-cpu=x86-64 -C target-feature=-avx,-avx2,-fma
```

That CI setting is a checked baseline for the Rust code tested here, not proof
that every supported compiler, CPU, or future distribution binary will produce
bit-identical floating-point results. The current source-level contract also
keeps `src/crop.rs` free of architecture intrinsics, runtime CPU feature
dispatch, `target_feature` attributes, and `f32::mul_add`; the integration test
`crop_sampler_retains_the_portable_cpu_operation_profile` guards that boundary.

This profile preserves the existing scalar operation order rather than trying
to select an OpenCV SIMD/FMA path. It does not establish universal OpenCV pixel
equivalence, select an optimization implementation, or relax the fourteen
reviewed fixture expectations. A future optimization proposal must update this
policy first, retain a portable baseline regression, and provide separately
reviewed numerical evidence before it changes the sampler.

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
