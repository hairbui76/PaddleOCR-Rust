# OpenCV Crop Oracle Capture

Roadmap items: `CROP-001`, `GEO-002`, `FIX-001`, `TOL-001`
Status: Two reviewed component captures are committed; the baseline capture has
an inverse-mapping sidecar. A separate narrow model-backed no-text capture is
recorded in `ORACLE_CAPTURE.md`, not in this component-crop record.
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

The project user also permits a repository-local, ignored `.oracle-venv/` for
developer-only capture work. It is not a Rust dependency, is never read by
normal Cargo commands or CI, and must not be committed, packaged, or used as an
asset-distribution mechanism.

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

The separate broad scalar grid is an explicit OpenCV configuration, not a
default-dispatch oracle. It selects the `scalar-grid` suite and turns off
OpenCV optimized paths before capture:

```sh
cd /path/outside/both-repositories
python3 /path/to/PaddleOCR-Rust/tools/capture_crop_oracle.py \
  --suite scalar-grid --disable-optimized > crop-scalar-grid.json
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
solving, plus a high-variation projective case whose scalar cubic conversion
requires nearest-even handling at a half-byte boundary. The corpus is BGR only because the frozen M2
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
`1ce072dd7633390302c674cac0cadfc574c7e8182d938625d9b0e3163a09cf3a`;
[metadata.json](../tests/fixtures/classic-v1-crop-oracle/metadata.json) records
the raw-byte aggregate hashes, upstream reference, review date, and limits.

The sidecar
[tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv](../tests/fixtures/classic-v1-crop-oracle/inverse-mappings.csv)
has SHA-256 `91a55b75910f3013d0c9405aefb6c3fd5a6b134f66a76f0027dd17c17475e3fa`.
It records seventy-five `warp → source` points: the four pre-rotation destination
boundaries and one interior coordinate for each of the fifteen reviewed cases.
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
`crop::tests::classic_crop_matches_perspective_lu_opencv_oracle_case`, and
`crop::tests::classic_crop_matches_ties_even_opencv_oracle_case`
check all fifteen recorded outputs without importing Python or OpenCV. Exact
agreement is evidence only for these self-authored BGR cases and this recorded
environment. It is not a claim of universal OpenCV interpolation parity,
upstream-environment parity, decoded-image behavior, or OCR compatibility.

`geometry::tests::classic_crop_plan_matches_recorded_opencv_interior_mappings`
also checks selected non-corner source-to-warp coordinates against
`cv2.perspectiveTransform` evaluations of the captured matrices for the phase,
one-pixel, and tall-thin cases. This is narrow mapping evidence for those
recorded matrices, not general OpenCV homography equivalence.

`geometry::tests::classic_crop_plan_matches_captured_opencv_inverse_mapping_oracle`
parses the sidecar offline and checks all seventy-five captured pre-rotation
warp-to-source coordinates against the private plan. It therefore covers the
mapping direction used by the crop sampler, while remaining limited to this
recorded OpenCV environment and the self-authored cases.

### Scalar-grid capture

The separate reviewed scalar capture is
[tests/fixtures/classic-v1-crop-scalar-grid/capture.json](../tests/fixtures/classic-v1-crop-scalar-grid/capture.json).
It was captured on 2026-08-03 with the same Python 3.12.3, NumPy 2.5.1,
OpenCV 5.0.0, and opencv-python-headless 5.0.0.93 environment, after the
generator called `cv2.setUseOptimized(False)`. Its exact JSON SHA-256 is
`6cad11b4d131d566ce4da8fc1eba5e0c2965972f4c1ee55463038606e3b84c09`.
The 36 self-authored BGR cases include the original 3–16-pixel coverage grid
plus one/two-pixel source axes, far replicated borders, low binary phase
boundaries, larger 17–31-pixel sources, and an exact output aspect ratio just
below the rotation boundary. They contain 9,744 input bytes, 7,293 output
bytes, and 14 post-warp rotations.

`crop::tests::classic_crop_executes_every_captured_opencv_scalar_grid_case`
checks every input, quadrilateral, pre-rotation dimensions, rotation decision,
and output byte array offline. The fixture metadata and
`tests/fixture_integrity.rs` pin its suite name, scalar setting, environment,
ordered IDs, per-payload hashes, and aggregate hashes. Exact agreement is only
evidence for these 36 self-authored cases and the recorded scalar environment;
it does not select an OpenCV dispatch policy or establish universal OpenCV,
decoder, model, or OCR equivalence.

## Scalar nearest-even rounding regression

A separate deterministic 1,024-case self-authored BGR probe (3–20 pixel
source sides, fixed LCG, and strictly convex projective quadrilaterals) found
seven one-byte differences from default OpenCV before the scalar conversion
change. The promoted `ties-even-bgr-4x7` case is one of those differences: its
expected byte is 132, while away-from-zero rounding produces 133. The private
sampler now calls `f32::round_ties_even()` after its explicit `[0, 255]`
saturation check; this matches the recorded OpenCV scalar behavior and the
selected fixture without introducing an intrinsic or CPU-feature dependency.

With that change, the complete 1,024-case probe matched
`cv2.setUseOptimized(False)` byte-for-byte. Default OpenCV still differed on
five isolated bytes, which is consistent with the already documented SIMD/FMA
variation. This observation is diagnostic only: no SIMD/FMA behavior, broader
tolerance, OpenCV universality, decoder behavior, or OCR compatibility is
claimed.

## Non-promoted optimization diagnostic

On 2026-08-02, before the scalar nearest-even correction, an isolated,
developer-only deterministic corpus of 4,096
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
behavior, and the reviewed fixture corpus remains the only exact pixel
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
The quality workflow runs the complete test suite in both the ordinary and
optimized `--release` profiles under the same portable flags, so recorded crop
and CTC regressions are checked against the profile used for distribution.

This profile preserves the existing scalar operation order rather than trying
to select an OpenCV SIMD/FMA path. It does not establish universal OpenCV pixel
equivalence, select an optimization implementation, or relax the fifteen
baseline or thirty-six scalar-grid fixture expectations. A future optimization
proposal must update this policy first, retain a portable baseline regression,
and provide separately reviewed numerical evidence before it changes the
sampler.

### Runtime no-AVX baseline evidence

On 2026-08-02, the current workspace commit
`8a5ccc79e6f2530748586ce5316402ec8b60bb85` built all present test binaries
outside the repository from its locked `Cargo.lock` SHA-256
`99492b0fe9d6c9ea6408ed53928708045e9386b15034d1cc876fea3a955b64dd`.
The Rust `1.94.0` release build used:

```text
-C target-cpu=x86-64 -C target-feature=+crt-static,-avx,-avx2,-fma
```

It produced static PIE library, binary, contract, and foundation test binaries
with SHA-256 values
`b3dff1b697d9f1d35fe5a05ba2116ccd00dea4c9670c0f3f8b3d65571c2786c5`,
`03623ec3927ae110d36264661b000e1f13bbf5c7de691e7af59a82140ab498d9`,
`531eaa37cf9e19ebaf6c73ac87a5f1233455c8400398f2ab54a4cda52e54ac07`,
and `360630797b8018e34ecb9236b755e62f415600497fa4c629a8e0f9bc1369cef6`.

Those binaries ran with one test thread inside a disposable QEMU `9.0.2` TCG
guest using one `qemu64` vCPU and 256 MiB memory. The guest's recorded
`/proc/cpuinfo` flags contain no `avx`, `avx2`, or `fma`. It passed all 65
library tests, the zero-test binary harness, three contract tests, and three
foundation tests; each process exited zero. The guest kernel came from the
external Ubuntu package
`linux-image-unsigned-7.0.0-28-generic` version `7.0.0-28.28~24.04.1`, package
SHA-256 `be2d970c035b7227362faa5972a3090cabb3cf6ad5284614ce98b2bd5f828f0a`.
The initramfs, kernel package, binary artifacts, and QEMU log were temporary
and were not added to this repository.

This is a single no-AVX/no-FMA runtime validation of the current offline test
suite, including the crop regressions. It does not prove all x86-64 hardware,
toolchains, optimized binaries, decoder paths, OpenCV equivalence, model
runtime behavior, or OCR support.

### Current full-suite QEMU replay (2026-08-03)

At workspace commit `986ea76cfbbda450970d3f8536bc4eac3f7ff125`, the complete
feature-enabled release test set was rebuilt outside the repository with Rust
`1.94.0` from `Cargo.lock` SHA-256
`e8fd73d88cd777d27419bf6c28412ac344bc30b87714057cfadde24752782c4c`:

```text
CARGO_TARGET_DIR=<temporary-target> \
RUSTFLAGS='-C target-cpu=x86-64 -C target-feature=+crt-static,-avx,-avx2,-fma' \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/gcc \
cargo test --locked --workspace --all-features --release --no-run
```

The resulting static PIE test binaries had SHA-256 values
`7e40c19ab9e3797a6b416737a1cbe1cf5aa5015ae0b17fd647b54997edb3e892`
(library), `03623ec3927ae110d36264661b000e1f13bbf5c7de691e7af59a82140ab498d9`
(binary harness), `4db1941fa726f56deb87d5a3dfb3f475f46a4da5378c5c785e1f97ec365ddbc5`
(contract), `cd29ebae1be6e58b98cc588b909e8a38f23af9a0eada60e8e54bad951b498639`
(fixture integrity), and
`c083466e204bc3e7fa2c8841cb42e0f35fdfaaad48b8baa44cbef634dcc403da`
(foundation).

A disposable initramfs mirrored the committed fixture tree only at the
compile-time workspace path required by `fixture_integrity`; it contained no
network device and no model. QEMU `9.0.2` TCG ran it with one `qemu64` vCPU,
256 MiB memory, and `-nic none`, using the external Alpine
`6.12.94-0-virt` kernel SHA-256
`12eb24189f3eb30bd0dcd919248caaa054ed4e87b799a53fdcc3999f157933e4`.
The initramfs and QEMU log SHA-256 values were respectively
`c35469494596a31b01a9dbb793b795cc05875f4a007e67214e7b7283f2fb37f9` and
`3491979e31950b48517b2c8d99886c8809e5aa5db9a8a9a4afbbe8556ec971de`.
Its guest `/proc/cpuinfo` flags omitted `avx`, `avx2`, and `fma`; all 82
library tests (including the four feature-gated fuzz regressions), the
zero-test binary harness, three contract tests, one fixture-integrity test,
and three foundation tests passed with `guest-result=0`, and QEMU exited zero.

The initramfs, kernel, test binaries, and log are temporary external evidence,
not repository artifacts. This is one emulated portable-baseline replay only;
it does not prove physical CPU support, all toolchains, all distribution
binaries, SIMD/dispatch behavior, decoder behavior, OpenCV equivalence, model
runtime behavior, or OCR support.

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
