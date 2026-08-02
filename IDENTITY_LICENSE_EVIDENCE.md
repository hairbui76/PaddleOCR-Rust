# Identity and License Evidence Record

Roadmap item: DEC-001 (decision-support evidence)
Status: Evidence complete; bootstrap decision recorded in P0_DECISIONS.md
Prepared: 2026-08-02
PaddleOCR baseline: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose

This record separates source-code licensing, attribution, branding, model
artifacts, datasets, fonts, and third-party runtime components before the Rust
project creates package metadata or distributes any asset. It is not legal
advice and does not grant rights to any upstream material.

The P0 evidence scan preceded the Rust workspace bootstrap. This record's
inventory statement therefore describes that scan rather than the repository's
current contents: at the time, it had no Rust source, copied upstream
implementation, model artifact, dataset, font, generated conversion output, or
chosen project license. The later project-license decision is recorded in
`P0_DECISIONS.md`; artifact-specific review is tracked by `LIC-001`.

## Read-only upstream evidence

| Evidence | Observed source | Meaning |
|---|---|---|
| Upstream source license | `PaddleOCR/LICENSE` identifies Apache License 2.0 and attributes PaddlePaddle Authors. | Apache-2.0 governs the upstream repository source under its terms. |
| Upstream project statement | `PaddleOCR/README.md` identifies the project as Apache-2.0. | Confirms the source-license presentation; it does not enumerate model-asset terms. |
| Trademark restriction | Apache-2.0 Section 6 in `PaddleOCR/LICENSE`. | The license does not grant permission to use licensor trade names, trademarks, service marks, or product names except customary attribution/origin description. |
| Attribution retention | Apache-2.0 Section 4 in `PaddleOCR/LICENSE`. | If adapted upstream source or notices are distributed, required notices and modification markings must be retained. |
| Third-party/deployment materials | Examples include `deploy/cpp_infer/THIRD_PARTY_LICENSES/` and platform demo subtrees. | Their licenses and native/runtime constraints cannot be inherited blindly by a Rust implementation. |
| Data/model/documentation references | Model and dataset download locations occur throughout `docs/`; dictionaries and fonts live under separate paths. | A repository source license alone is insufficient evidence that individual weights, datasets, fonts, or converted artifacts are redistributable. |

No root-level `NOTICE` file was found in the inspected upstream checkout. This
does not remove the obligation to retain notices if a particular copied or
adapted source file carries one, or if a third-party dependency requires one.

## Consequences for this project

| Material | Current project rule | Evidence required before use or distribution |
|---|---|---|
| Original Rust source | May use the project's eventual selected license. | Project owner and license decision. |
| Adapted upstream source or non-trivial translations | Avoid unless necessary; preserve applicable copyright/license/notice text and identify modifications. | File-level provenance and license review. |
| Public API names or behavioral descriptions | May describe compatibility factually. | Wording must not imply an official PaddlePaddle/PaddleOCR release or endorsement. |
| Name, logo, or branding | Do not select or use an official-looking name, logo, or claim of affiliation by default. | Explicit branding review and, if necessary, permission. |
| Model weights and converted model files | Do not bundle, mirror, or redistribute by default. | Artifact-specific terms, source URL, hash, conversion provenance, and distribution approval. |
| Dictionaries, fonts, fixtures, and datasets | Do not copy by default. | Asset-specific license/provenance and test-fixture approval. |
| Rust crates and native libraries | Select only after the runtime/decoder decision. | Per-dependency license, platform, `unsafe`, and notice review. |

## Non-binding project-license direction

Apache-2.0 is a plausible candidate for the Rust project because it is
compatible with the upstream source license and clearly accommodates retained
notices for any future adapted material. It is not selected by this record.

The following conditions remain regardless of the eventual project-source
license:

1. Do not represent the project as an official PaddlePaddle or PaddleOCR
   release.
2. Do not use upstream logos or suggest sponsorship/endorsement without
   permission.
3. Maintain a provenance record for every non-trivial copied/adapted source or
   bundled asset.
4. Treat weights, datasets, fonts, dictionaries, and conversion output as
   separately licensed until their terms are verified.
5. Include required third-party notices in a release when the selected
   dependencies or adapted material require them.

## Decisions still required

| Decision | Must be supplied or approved | Why it cannot be inferred |
|---|---|---|
| Project identity | Package/repository display name, crate name, owner/copyright holder, and non-affiliation wording. | Crate availability, ownership, and branding risk are external facts and policy choices. |
| Project source license | A specific SPDX expression and whether dual licensing is intended. | Apache compatibility does not dictate the project's chosen terms. |
| Attribution policy | Location and format for upstream/third-party attribution, if material is adapted. | It depends on material actually retained. |
| Model/asset distribution policy | Local-only, opt-in download, or approved bundled artifacts. | It depends on individual asset terms and release intent. |
| Contribution policy | Whether contributors must sign a CLA/DCO or follow another policy. | It is a project-governance choice, not an upstream requirement for an independent port. |

## Completion condition for DEC-001

`DEC-001` can become Done only after the user approves the project identity and
license direction, a concrete license file and attribution policy are created
in this repository, and the roadmap records the decision. Asset-specific
reviews remain separate work under `LIC-001` and must precede any artifact
distribution.
