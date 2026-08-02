# PaddleOCR Upstream Capability Inventory

Status: Done for INV-001 baseline inventory
Inventory version: 0.1.0
Inspection date: 2026-08-02
Upstream commit: 2661c7c0ef5c613e8f93c6e93b2e052399f0f854

## Purpose and authority

This is a factual inventory of the local read-only PaddleOCR checkout at the
pinned commit. It is the detailed input to SCOPE-001 and SCOPE-002. It is not a
Rust support matrix, a runtime choice, a model-distribution decision, or a claim
that every listed surface will be ported in the same form.

The root `ROADMAP.md` governs sequencing and completion. `COMPATIBILITY.md`, created after
scope approval, will classify every in-scope capability by priority and record
the exact Rust surface, model artifact, fixture, tolerance, and evidence.

All paths below are relative to the upstream checkout reached through
./PaddleOCR. They are references only. Never modify them or make Rust builds,
tests, packages, or runtime depend on them.

## Inventory method and limits

- Public module and pipeline rows come from paddleocr/__init__.py,
  paddleocr/_models/__init__.py, paddleocr/_pipelines/__init__.py, and
  paddleocr/_cli.py.
- Default model names and CLI subcommands come from each wrapper source file.
- Classic algorithm and configuration rows come from configs/, ppocr/, tools/,
  and ppstructure/.
- Test, deployment, browser, API SDK, MCP, and LangChain rows come from their
  corresponding directories.
- The modern Python facade delegates substantial behavior to PaddleX. This
  checkout's paddleocr/ source alone is not enough to assert full
  modern-pipeline semantics; BASE-002 must pin the relevant PaddleX
  version/source or record black-box contract evidence before compatibility is
  claimed.
- Counts describe files present at this commit, not independently verified
  supported models, quality, licenses, or distribution rights.

## Baseline summary

| Surface | Count at baseline | Primary source |
|---|---:|---|
| Modern public model wrappers | 13 | paddleocr/_models/ |
| Modern public pipeline wrappers | 10 | paddleocr/_pipelines/ |
| Training configuration files | 155 | configs/ |
| Classifier configurations | 2 | configs/cls/ |
| Detection configurations | 35 | configs/det/ |
| End-to-end spotting configurations | 1 | configs/e2e/ |
| KIE configurations | 10 | configs/kie/ |
| Recognition/formula configurations | 98 | configs/rec/ |
| Super-resolution configurations | 2 | configs/sr/ |
| Table configurations | 7 | configs/table/ |
| Upstream test modules named test_*.py | 36 | tests/ |
| Small upstream image fixtures | 9 | tests/test_files/ |
| Dictionary/tokenizer data files | 65 | ppocr/utils/dict/ |
| Visualization font files | 19 | doc/fonts/ |

## A. Modern public library surface

### A.1 Public standalone models

All rows are exported by paddleocr/__init__.py and registered by
paddleocr/_cli.py. Rust status is intentionally Unassessed for every row.

| ID | Python class | CLI subcommand | Default model | Wrapper family | Test/reference | Track | Rust status |
|---|---|---|---|---|---|---|---|
| UP-MOD-001 | TextDetection | text_detection | PP-OCRv6_medium_det | text-detection/PaddleX predictor | _models/text_detection.py; tests/models/test_text_detection.py | P5 | Unassessed |
| UP-MOD-002 | TextRecognition | text_recognition | PP-OCRv6_medium_rec | PaddleX predictor | _models/text_recognition.py; tests/models/test_text_recognition.py | P5 | Unassessed |
| UP-MOD-003 | TextLineOrientationClassification | textline_orientation_classification | PP-LCNet_x0_25_textline_ori | image classification | _models/textline_orientation_classification.py; tests/models/test_textline_orientation_classification.py | P5 | Unassessed |
| UP-MOD-004 | DocImgOrientationClassification | doc_img_orientation_classification | PP-LCNet_x1_0_doc_ori | image classification | _models/doc_img_orientation_classification.py; tests/models/test_doc_img_orientation_classification.py | P7 | Unassessed |
| UP-MOD-005 | TextImageUnwarping | text_image_unwarping | UVDoc | PaddleX predictor | _models/text_image_unwarping.py; tests/models/test_text_image_unwarping.py | P7 | Unassessed |
| UP-MOD-006 | LayoutDetection | layout_detection | PP-DocLayout_plus-L | object detection | _models/layout_detection.py; tests/models/test_layout_detection.py | P8 | Unassessed |
| UP-MOD-007 | TableClassification | table_classification | PP-LCNet_x1_0_table_cls | image classification | _models/table_classification.py; tests/models/test_table_classification.py | P8 | Unassessed |
| UP-MOD-008 | TableCellsDetection | table_cells_detection | RT-DETR-L_wired_table_cell_det | object detection | _models/table_cells_detection.py; tests/models/test_table_cells_detection.py | P8 | Unassessed |
| UP-MOD-009 | TableStructureRecognition | table_structure_recognition | SLANet | PaddleX predictor | _models/table_structure_recognition.py; tests/models/test_table_structure_recognition.py | P8 | Unassessed |
| UP-MOD-010 | FormulaRecognition | formula_recognition | PP-FormulaNet_plus-M | PaddleX predictor | _models/formula_recognition.py; tests/models/test_formula_recognition.py | P8/P12 | Unassessed |
| UP-MOD-011 | SealTextDetection | seal_text_detection | PP-OCRv4_mobile_seal_det | text-detection/PaddleX predictor | _models/seal_text_detection.py; tests/models/test_seal_text_detection.py | P8 | Unassessed |
| UP-MOD-012 | ChartParsing | chart_parsing | PP-Chart2Table | document VLM | _models/chart_parsing.py | P8/P10 | Unassessed |
| UP-MOD-013 | DocVLM | doc_vlm | PP-DocBee2-3B | document VLM | _models/doc_vlm.py; tests/models/test_doc_vlm.py | P10 | Unassessed |

Observed result families to freeze in P2:

| Family | Observed fields/examples | Test/helper reference |
|---|---|---|
| Text detection | input_path, page_index, input_img, dt_polys, dt_scores | tests/models/test_text_detection.py |
| Text recognition | rec_text, rec_score, visualization font metadata | tests/models/test_text_recognition.py |
| Image classification | class_ids, scores, label_names | tests/models/image_classification_common.py |
| Object/layout/cell detection | boxes | tests/models/object_detection_common.py |
| Table structure | bbox, structure, structure_score | tests/models/test_table_structure_recognition.py |
| Text image unwarping | doctr_img | tests/models/test_text_image_unwarping.py |
| Formula recognition | rec_formula | tests/models/test_formula_recognition.py |
| Document VLM/chart parsing | structured result | tests/models/test_doc_vlm.py; module documentation |

The result-field list is an inventory aid, not a frozen JSON contract. P2 must
record nesting, types, nullability, ordering, coordinate units, and errors.

### A.2 Public pipelines

| ID | Python class | CLI subcommand | PaddleX pipeline name | Main composition/switches | Test reference | Track | Rust status |
|---|---|---|---|---|---|---|---|
| UP-PIPE-001 | PaddleOCR | ocr | OCR | doc orientation, unwarping, text-line orientation, detection, crop/sort, recognition | tests/pipelines/test_ocr.py | P6 | Unassessed |
| UP-PIPE-002 | DocPreprocessor | doc_preprocessor | doc_preprocessor | doc orientation and unwarping | tests/pipelines/test_doc_preprocessor.py | P7 | Unassessed |
| UP-PIPE-003 | FormulaRecognitionPipeline | formula_recognition_pipeline | formula_recognition | doc preprocess, optional layout, formula recognition | tests/pipelines/test_formula_recognition.py | P9 | Unassessed |
| UP-PIPE-004 | SealRecognition | seal_recognition | seal_recognition | doc preprocess, layout, seal detection/recognition | tests/pipelines/test_seal_rec.py | P9 | Unassessed |
| UP-PIPE-005 | TableRecognitionPipelineV2 | table_recognition_v2 | table_recognition_v2 | doc preprocess, layout, OCR, table classification/structure/cells, matching | tests/pipelines/test_table_recognition_v2.py | P9 | Unassessed |
| UP-PIPE-006 | PPStructureV3 | pp_structurev3 | PP-StructureV3 | doc preprocess, OCR, layout, seal, table, formula, chart | tests/pipelines/test_pp_structurev3.py | P9 | Unassessed |
| UP-PIPE-007 | PaddleOCRVL | doc_parser | PaddleOCR-VL, PaddleOCR-VL-1.5, PaddleOCR-VL-1.6 | doc preprocess, layout, VLM, optional chart/seal/image OCR | no same-named pipeline test observed | P10 | Unassessed |
| UP-PIPE-008 | DocUnderstanding | doc_understanding | doc_understanding | image plus query document VLM | tests/pipelines/test_doc_understanding.py | P10 | Unassessed |
| UP-PIPE-009 | PPChatOCRv4Doc | pp_chatocrv4_doc | PP-ChatOCRv4-doc | layout parsing, vector build/retrieval, MLLM/LLM chat | tests/pipelines/test_pp_chatocrv4_doc.py | P10 | Unassessed |
| UP-PIPE-010 | PPDocTranslation | pp_doctranslation | PP-DocTranslation | structure/layout parsing, Markdown, translation/provider stages | tests/pipelines/test_pp_doctranslation.py | P10 | Unassessed |

The modern wrapper sources are mostly configuration and API adapters around
PaddleX. The following are stronger independent references for P5/P6:

- deploy/cpp_infer/src/pipelines/ocr/
- deploy/cpp_infer/src/pipelines/doc_preprocessor/
- paddleocr-js/packages/core/src/models/
- paddleocr-js/packages/core/src/pipelines/ocr/

### A.3 Public utility, API-client, and CLI surface

| ID | Surface | Upstream source | Inventory scope | Track | Rust status |
|---|---|---|---|---|---|
| UP-UTIL-001 | doc2md_convert and doc2md_supported_formats | paddleocr/__init__.py; _doc2md/ | DOCX/XLSX/PPTX conversion/options | P9 | Unassessed |
| UP-UTIL-002 | Benchmark export | paddleocr/__init__.py | public benchmark helper | P13 | Unassessed |
| UP-UTIL-003 | Logger export | paddleocr/__init__.py; _utils/logging.py | public logging behavior | P1/P11 | Unassessed |
| UP-API-001 | Sync API client | _api_client/client.py | job submission, polling, results/resources | P11 | Unassessed |
| UP-API-002 | Async API client | _api_client/async_client.py | async job submission, polling, results/resources | P11 | Unassessed |
| UP-API-003 | API models/options/errors | _api_client/{models,errors,results}.py | model enum, OCR/structure/VL options, typed failures | P11 | Unassessed |
| UP-CLI-001 | Model and pipeline commands | _cli.py | all rows in A.1 and A.2 | P6/P11 | Unassessed |
| UP-CLI-002 | doc2md | _cli.py | office input/output/options | P9 | Unassessed |
| UP-CLI-003 | api | _cli.py; _api_client/cli.py | remote API client command | P11 | Unassessed |
| UP-CLI-004 | genai_server | _cli.py | PaddleX GenAI server invocation | P10/P11 | Unassessed |
| UP-CLI-005 | install_hpi_deps and install_genai_server_deps | _cli.py | Python/PaddleX installer behavior; not a Rust runtime implementation | P11 decision | Unassessed |

The exported API error taxonomy includes AuthError, InvalidRequestError,
APIError, JobFailedError, RateLimitError, RequestTimeoutError, PollTimeoutError,
ResponseFormatError, ResultParseError, ServiceUnavailableError, and NetworkError.
A Rust equivalent need not copy the class hierarchy, but P11 must document any
observable mapping difference.

## B. Classic OCR inference semantics and source map

### B.1 Priority reference path for a classic OCR vertical slice

The baseline classic path supplies observable behavior to capture before P5/P6:

1. tools/infer/predict_det.py resizes/normalizes detector input and performs
   model inference.
2. ppocr/postprocess/db_postprocess.py turns detector maps into polygons or
   quadrilaterals using thresholds, scoring, expansion, clipping, and inverse
   resize mapping.
3. tools/infer/predict_system.py sorts boxes in reading order, performs
   perspective crops, conditionally rotates tall crops, optionally classifies
   orientation, batches recognizer crops by aspect ratio, restores order, and
   filters scores.
4. tools/infer/predict_rec.py normalizes/pads recognizer inputs and batches
   them.
5. ppocr/postprocess/rec_postprocess.py decodes text, handles blank/repeated
   tokens, uses the ordered dictionary ABI, and produces confidence values.

Supporting geometric helpers live in tools/infer/utility.py. P2 must capture
selected-model defaults, including thresholds, image shape, resize policy, text
normalization, point order, rounding, and language rules. Current code includes
Arabic and model-specific dictionary behavior; do not generalize it without
evidence.

### B.2 Inference and training script inventory

| ID | Script family | Upstream paths | Scope represented | Track |
|---|---|---|---|---|
| UP-TOOL-001 | Classic inference | tools/infer/predict_{cls,det,e2e,rec,sr,system}.py | classifier, detector, spotting, recognizer, SR, full OCR | P5/P6/P12 |
| UP-TOOL-002 | Legacy inference entrypoints | tools/infer_{cls,det,e2e,kie,kie_token_ser,kie_token_ser_re,rec,sr,table}.py | task-specific command behavior | P8/P12 |
| UP-TOOL-003 | Train/eval/export | tools/{train,eval,program,export_model,export_center}.py | lifecycle, training, evaluation, export | P12 |
| UP-TOOL-004 | End-to-end utilities | tools/end2end/{convert_ppocr_label,draw_html,eval_end2end}.py | label conversion, visual evaluation | P12 |
| UP-TOOL-005 | Documentation/config safety | tools/check_docs_github_links.py; tools/resolve_doc_github_refs.py; tests/tools/test_program_safe_yaml.py | documentation and safe configuration parsing | P1/P13 |

### B.3 Algorithm implementation families

| ID | Family | Important upstream sources | Baseline subfamilies | Track | Rust status |
|---|---|---|---|---|---|
| UP-ALG-001 | Detection | ppocr/modeling/{backbones,necks,heads}/det_*.py; ppocr/postprocess/*_postprocess.py | DB/DB++, EAST, SAST, PSE, FCE, CT, DRRG | P5 DB; P12 breadth | Unassessed |
| UP-ALG-002 | End-to-end spotting | e2e_resnet_vd_pg.py; det_pg_head.py; pg_fpn.py; pg_postprocess.py | PGNet | P12 | Unassessed |
| UP-ALG-003 | Text recognition | ppocr/modeling/backbones/rec_*.py; heads/rec_*.py; postprocess/rec_postprocess.py | CTC/CRNN/PP-OCR/SVTR, attention, SAR, SRN, NRTR, ABINet, VisionLAN, RobustScanner, SPIN, RFL, SATRN, ParseQ, CPPD | P5 CTC; P12 breadth | Unassessed |
| UP-ALG-004 | Formula recognition | formula backbones/heads/losses and configs/rec/{LaTeX_OCR_rec.yaml,UniMERNet.yaml,PP-FormuaNet/} | CAN, LaTeXOCR, UniMERNet, PP-FormulaNet | P8/P12 | Unassessed |
| UP-ALG-005 | Classification | heads/cls_head.py; postprocess/cls_postprocess.py; tools/infer/predict_cls.py | document/text-line orientation | P5/P7/P12 | Unassessed |
| UP-ALG-006 | Table | table backbones/heads/necks/postprocess and ppstructure/table/ | SLANet, SLANeXt, TableMaster, matching/HTML | P8/P9/P12 | Unassessed |
| UP-ALG-007 | KIE | backbones/vqa_layoutlm.py; heads/kie_sdmgr_head.py; ppstructure/kie/ | SDMGR, LayoutLM, LayoutLMv2, LayoutXLM, VI-LayoutXLM SER/RE | P8/P12 | Unassessed |
| UP-ALG-008 | Super-resolution | transforms/{tbsrn,tsrn}.py; SR heads/losses; configs/sr/ | TSRN, TBSRN, Telescope | P8/P12 | Unassessed |
| UP-ALG-009 | Core composition | modeling/architectures/{base_model,distillation_model}.py | Transform -> Backbone -> Neck -> Head and distillation | P12 | Unassessed |
| UP-ALG-010 | Data/training infrastructure | ppocr/data/; ppocr/losses/; ppocr/metrics/; ppocr/optimizer/ | datasets, augmentation, losses, metrics, optimizers, schedulers | P12 | Unassessed |

### B.4 Data, augmentation, loss, metric, and optimizer inventory

| Surface | Upstream paths | Baseline contents |
|---|---|---|
| Dataset loaders | ppocr/data/{simple_dataset,lmdb_dataset,multi_scale_sampler,pgnet_dataset,pubtab_dataset,latexocr_dataset}.py | simple, LMDB, multi-scale, PGNet, PubTab, LaTeX/OCR data |
| Collation | ppocr/data/collate_fn.py | batch collation |
| Image/label augmentation | ppocr/data/imaug/ | color jitter, crop/paste, detection target maps, recognition/table/LaTeX augmentation, RandAugment, label operators |
| Detection postprocessing | ppocr/postprocess/{db,ct,drrg,east,fce,pse,sast}_postprocess.py | task-specific detector decoding |
| Recognition/table/KIE decoding | ppocr/postprocess/{rec,table,vqa_token_re_layoutlm,vqa_token_ser_layoutlm}_postprocess.py | text, table structure, KIE decoding |
| Losses | ppocr/losses/ | 42 Python modules across CTC/attention/detection/table/KIE/SR/distillation families |
| Metrics | ppocr/metrics/ | 14 modules for detection, recognition, formula, table, KIE, SR, and end-to-end metrics |
| Optimizers/schedulers | ppocr/optimizer/ | optimizer, regularizer, learning-rate scheduler support |

No arbitrary Python dataset transform, pickle, YAML object, or checkpoint format
may be carried into Rust by deserialization. Existing security tests
tests/security/test_latexocr_pickle.py, tests/security/test_lmdb_pickle.py, and
tests/tools/test_program_safe_yaml.py identify relevant input boundaries.

## C. Configuration inventory

The following paths are relative to PaddleOCR/configs/. They are an inventory of
training/configuration declarations, not a promise that models, datasets, or
weights are available or redistributable.

### C.1 Classification: 2 configurations

~~~text
cls/ch_PP-OCRv3/ch_PP-OCRv3_rotnet.yml
cls/cls_mv3.yml
~~~

### C.2 Detection: 35 configurations

~~~text
det/PP-OCRv3/PP-OCRv3_det_cml.yml
det/PP-OCRv3/PP-OCRv3_det_dml.yml
det/PP-OCRv3/PP-OCRv3_mobile_det.yml
det/PP-OCRv3/PP-OCRv3_server_det.yml
det/PP-OCRv4/PP-OCRv4_det_cml.yml
det/PP-OCRv4/PP-OCRv4_mobile_det.yml
det/PP-OCRv4/PP-OCRv4_mobile_seal_det.yml
det/PP-OCRv4/PP-OCRv4_server_det.yml
det/PP-OCRv4/PP-OCRv4_server_seal_det.yml
det/PP-OCRv5/PP-OCRv5_mobile_det.yml
det/PP-OCRv5/PP-OCRv5_server_det.yml
det/PP-OCRv6/PP-OCRv6_medium_det.yml
det/PP-OCRv6/PP-OCRv6_small_det.yml
det/PP-OCRv6/PP-OCRv6_tiny_det.yml
det/ch_PP-OCRv2/ch_PP-OCRv2_det_cml.yml
det/ch_PP-OCRv2/ch_PP-OCRv2_det_distill.yml
det/ch_PP-OCRv2/ch_PP-OCRv2_det_dml.yml
det/ch_PP-OCRv2/ch_PP-OCRv2_det_student.yml
det/ch_ppocr_v2.0/ch_det_mv3_db_v2.0.yml
det/ch_ppocr_v2.0/ch_det_res18_db_v2.0.yml
det/det_mv3_db.yml
det/det_mv3_east.yml
det/det_mv3_pse.yml
det/det_r18_vd_ct.yml
det/det_r50_db++_icdar15.yml
det/det_r50_db++_td_tr.yml
det/det_r50_drrg_ctw.yml
det/det_r50_vd_db.yml
det/det_r50_vd_dcn_fce_ctw.yml
det/det_r50_vd_east.yml
det/det_r50_vd_pse.yml
det/det_r50_vd_sast_icdar15.yml
det/det_r50_vd_sast_totaltext.yml
det/det_repsvtr_db.yml
det/det_res18_db_v2.0.yml
~~~

### C.3 End-to-end spotting: 1 configuration

~~~text
e2e/e2e_r50_vd_pg.yml
~~~

### C.4 KIE: 10 configurations

~~~text
kie/layoutlm_series/re_layoutlmv2_xfund_zh.yml
kie/layoutlm_series/re_layoutxlm_xfund_zh.yml
kie/layoutlm_series/ser_layoutlm_xfund_zh.yml
kie/layoutlm_series/ser_layoutlmv2_xfund_zh.yml
kie/layoutlm_series/ser_layoutxlm_xfund_zh.yml
kie/sdmgr/kie_unet_sdmgr.yml
kie/vi_layoutxlm/re_vi_layoutxlm_xfund_zh.yml
kie/vi_layoutxlm/re_vi_layoutxlm_xfund_zh_udml.yml
kie/vi_layoutxlm/ser_vi_layoutxlm_xfund_zh.yml
kie/vi_layoutxlm/ser_vi_layoutxlm_xfund_zh_udml.yml
~~~

### C.5 Recognition and formula: 98 configurations

~~~text
rec/LaTeX_OCR_rec.yaml
rec/PP-FormuaNet/PP-FormulaNet-L.yaml
rec/PP-FormuaNet/PP-FormulaNet-S.yaml
rec/PP-FormuaNet/PP-FormulaNet_plus-L.yaml
rec/PP-FormuaNet/PP-FormulaNet_plus-M.yaml
rec/PP-FormuaNet/PP-FormulaNet_plus-S.yaml
rec/PP-OCRv3/PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/PP-OCRv3_mobile_rec_distillation.yml
rec/PP-OCRv3/en_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/arabic_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/chinese_cht_PP-OCRv3_mobile_rec.yaml
rec/PP-OCRv3/multi_language/cyrillic_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/devanagari_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/japan_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/ka_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/korean_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/latin_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/ta_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv3/multi_language/te_PP-OCRv3_mobile_rec.yml
rec/PP-OCRv4/PP-OCRv4_mobile_rec.yml
rec/PP-OCRv4/PP-OCRv4_mobile_rec_ampO2_ultra.yml
rec/PP-OCRv4/PP-OCRv4_mobile_rec_distillation.yml
rec/PP-OCRv4/PP-OCRv4_mobile_rec_fp32_ultra.yml
rec/PP-OCRv4/PP-OCRv4_server_rec.yml
rec/PP-OCRv4/PP-OCRv4_server_rec_ampO2_ultra.yml
rec/PP-OCRv4/PP-OCRv4_server_rec_doc.yml
rec/PP-OCRv4/PP-OCRv4_server_rec_fp32_ultra.yml
rec/PP-OCRv4/ch_PP-OCRv4_rec_svtr_large.yml
rec/PP-OCRv4/en_PP-OCRv4_mobile_rec.yml
rec/PP-OCRv5/PP-OCRv5_mobile_rec.yml
rec/PP-OCRv5/PP-OCRv5_server_rec.yml
rec/PP-OCRv5/multi_language/arabic_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/cyrillic_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/devanagari_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/el_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/en_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/eslav_PP-OCRv5_mobile_rec.yml
rec/PP-OCRv5/multi_language/korean_PP-OCRv5_mobile_rec.yml
rec/PP-OCRv5/multi_language/latin_PP-OCRv5_mobile_rec.yml
rec/PP-OCRv5/multi_language/ta_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/te_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv5/multi_language/th_PP-OCRv5_mobile_rec.yaml
rec/PP-OCRv6/PP-OCRv6_medium_rec.yml
rec/PP-OCRv6/PP-OCRv6_small_rec.yml
rec/PP-OCRv6/PP-OCRv6_tiny_rec.yml
rec/SVTRv2/ch_RepSVTR_rec.yml
rec/SVTRv2/ch_RepSVTR_rec_gtc.yml
rec/SVTRv2/ch_SVTRv2_rec.yml
rec/SVTRv2/ch_SVTRv2_rec_distillation.yml
rec/SVTRv2/ch_SVTRv2_rec_gtc.yml
rec/SVTRv2/ch_SVTRv2_rec_gtc_distill.yml
rec/UniMERNet.yaml
rec/ch_PP-OCRv2/ch_PP-OCRv2_rec.yml
rec/ch_PP-OCRv2/ch_PP-OCRv2_rec_distillation.yml
rec/ch_PP-OCRv2/ch_PP-OCRv2_rec_enhanced_ctc_loss.yml
rec/ch_ppocr_v2.0/rec_chinese_common_train_v2.0.yml
rec/ch_ppocr_v2.0/rec_chinese_lite_train_v2.0.yml
rec/multi_language/rec_arabic_lite_train.yml
rec/multi_language/rec_cyrillic_lite_train.yml
rec/multi_language/rec_devanagari_lite_train.yml
rec/multi_language/rec_en_number_lite_train.yml
rec/multi_language/rec_french_lite_train.yml
rec/multi_language/rec_german_lite_train.yml
rec/multi_language/rec_hebrew_lite_train.yml
rec/multi_language/rec_japan_lite_train.yml
rec/multi_language/rec_korean_lite_train.yml
rec/multi_language/rec_latin_lite_train.yml
rec/multi_language/rec_multi_language_lite_train.yml
rec/multi_language/rec_samaritan_lite_train.yml
rec/multi_language/rec_syriac_lite_train.yml
rec/rec_d28_can.yml
rec/rec_efficientb3_fpn_pren.yml
rec/rec_icdar15_train.yml
rec/rec_mtb_nrtr.yml
rec/rec_mv3_none_bilstm_ctc.yml
rec/rec_mv3_none_none_ctc.yml
rec/rec_mv3_tps_bilstm_att.yml
rec/rec_mv3_tps_bilstm_ctc.yml
rec/rec_r31_robustscanner.yml
rec/rec_r31_sar.yml
rec/rec_r32_gaspin_bilstm_att.yml
rec/rec_r34_vd_none_bilstm_ctc.yml
rec/rec_r34_vd_none_none_ctc.yml
rec/rec_r34_vd_tps_bilstm_att.yml
rec/rec_r34_vd_tps_bilstm_ctc.yml
rec/rec_r45_abinet.yml
rec/rec_r45_visionlan.yml
rec/rec_r50_fpn_srn.yml
rec/rec_resnet_rfl_att.yml
rec/rec_resnet_rfl_visual.yml
rec/rec_resnet_stn_bilstm_att.yml
rec/rec_satrn.yml
rec/rec_svtrnet.yml
rec/rec_svtrnet_ch.yml
rec/rec_svtrnet_cppd_base_ch.yml
rec/rec_svtrnet_cppd_base_en.yml
rec/rec_vit_parseq.yml
rec/rec_vitstr_none_ce.yml
~~~

### C.6 Super-resolution: 2 configurations

~~~text
sr/sr_telescope.yml
sr/sr_tsrn_transformer_strock.yml
~~~

### C.7 Table: 7 configurations

~~~text
table/SLANeXt_wired.yml
table/SLANeXt_wireless.yml
table/SLANet.yml
table/SLANet_lcnetv2.yml
table/SLANet_plus.yml
table/table_master.yml
table/table_mv3.yml
~~~

## D. Legacy document structure and recovery surface

| ID | Surface | Upstream paths | Role | Track | Rust status |
|---|---|---|---|---|---|
| UP-STRUCT-001 | Legacy structure pipeline | ppstructure/predict_system.py; ppstructure/utility.py | layout -> OCR -> table/formula/KIE orchestration | P9 | Unassessed |
| UP-STRUCT-002 | Layout prediction | ppstructure/layout/predict_layout.py | legacy layout analysis | P8/P9 | Unassessed |
| UP-STRUCT-003 | Table structure/matching | ppstructure/table/{predict_structure,predict_table,matcher,table_master_match,convert_label2html,eval_table}.py | structure tokens, text/cell matching, HTML/evaluation | P8/P9/P12 | Unassessed |
| UP-STRUCT-004 | KIE prediction | ppstructure/kie/{predict_kie_token_ser,predict_kie_token_ser_re}.py | SER/RE application behavior | P8/P12 | Unassessed |
| UP-STRUCT-005 | PDF to Word | ppstructure/pdf2word/pdf2word.py | document conversion | P7/P9 decision | Unassessed |
| UP-STRUCT-006 | Recovery/export | ppstructure/recovery/{recovery_to_doc,recovery_to_markdown,table_process}.py | DOCX/Markdown/table reconstruction | P9 | Unassessed |

## E. Deployment, alternate runtime, and integration surface

| ID | Surface | Upstream paths | Inventory notes | Track | Rust status |
|---|---|---|---|---|---|
| UP-DEP-001 | Current native C++ inference | deploy/cpp_infer/ | OCR/document-preprocessor for text detection/recognition, doc orientation, unwarping, text-line orientation; independent contract reference | P3-P7 | Unassessed |
| UP-DEP-002 | ONNX conversion | deploy/paddle2onnx/ | conversion/deployment path; artifact provenance/drift need review | P3/P12 | Unassessed |
| UP-DEP-003 | Paddle Lite | deploy/lite/ | lightweight/mobile deployment | P11 | Unassessed |
| UP-DEP-004 | Android | deploy/android_demo/; deploy/ppocr-android/ | Android demo/SDK packaging | P11 | Unassessed |
| UP-DEP-005 | iOS | deploy/ios_demo/ | iOS demo/test assets | P11 | Unassessed |
| UP-DEP-006 | Bare-metal accelerator | deploy/avh/ | AVH-specific deployment | P11 | Unassessed |
| UP-DEP-007 | HubServing | deploy/hubserving/ | OCR, det/rec/cls, layout, table, KIE service modules | P11 | Unassessed |
| UP-DEP-008 | Docker/PaddleCloud | deploy/docker/; deploy/paddlecloud/ | container/cloud deployment | P11 | Unassessed |
| UP-DEP-009 | VLM high-performance server | deploy/paddleocr_vl_docker/ | accelerator/server gateway deployment | P10/P11 | Unassessed |
| UP-DEP-010 | Model slimming | deploy/slim/{auto_compression,prune,quantization}/ | compression/quantization flows | P12 | Unassessed |
| UP-WEB-001 | Browser SDK | paddleocr-js/packages/core/ | ONNX Runtime Web/OpenCV.js reference for DB+CTC, model assets, workers, visualization | P3/P5/P6/P11 | Unassessed |
| UP-API-004 | Go cloud SDK | api_sdk/go/ | typed cloud API reference | P11 | Unassessed |
| UP-API-005 | TypeScript cloud SDK | api_sdk/typescript/ | typed cloud API reference | P11 | Unassessed |
| UP-ECO-001 | MCP server | mcp_server/ | provider/task integration for OCR/document parsing | P11 | Unassessed |
| UP-ECO-002 | LangChain package | langchain-paddleocr/ | PaddleOCR-VL document loader integration | P11 | Unassessed |

The browser core contains models/{det,rec,infer}.ts,
pipelines/ocr/{core,crop,config,default-config,runtime-params}.ts, model-asset
and tar handling, ONNX/OpenCV runtime adapters, worker protocol, and rendering.
It is an independent implementation reference, not a Rust dependency or a
license waiver for bundled models/assets.

## F. Tests and fixture inventory

### F.1 Modern facade and pipeline tests

| Test group | Upstream test files |
|---|---|
| API client | api_client/test_{cli,core,http,resources}.py |
| Model wrappers | models/test_{doc_img_orientation_classification,doc_vlm,formula_recognition,layout_detection,seal_text_detection,table_cells_detection,table_classification,table_structure_recognition,text_detection,text_image_unwarping,text_recognition,textline_orientation_classification}.py |
| Pipelines | pipelines/test_{doc_preprocessor,doc_understanding,formula_recognition,ocr,pp_chatocrv4_doc,pp_doctranslation,pp_structurev3,seal_rec,table_recognition_v2}.py |
| Shared model assertions | models/{image_classification_common,object_detection_common}.py |

### F.2 Classic, safety, and regression tests

| Test group | Upstream test files |
|---|---|
| PPOCR postprocess/model | ppocr/test_{cls_postprocess,formula_model,iaa_augment,rec_postprocess}.py |
| Structure | test_ppstructure.py; unit/test_patch_layout_parsing.py |
| Security | security/test_{latexocr_pickle,lmdb_pickle}.py |
| Tool/config safety | tools/test_{docs_github_links,program_safe_yaml}.py |
| Naming/utilities | utils/test_naming.py |

### F.3 Small fixture files

~~~text
tests/test_files/book.jpg
tests/test_files/book_rot180.jpg
tests/test_files/doc_with_formula.png
tests/test_files/formula.png
tests/test_files/medal_table.png
tests/test_files/seal.png
tests/test_files/table.jpg
tests/test_files/textline.png
tests/test_files/textline_rot180.jpg
~~~

These files are not automatically cleared for redistribution. FIX-001 must
verify provenance/terms before copying any into this repository; otherwise,
create new legal fixtures that exercise the same contract.

## G. Asset, model, and packaging inventory

### G.1 Dictionaries and tokenizers

These files are model-coupled ordered data. A Rust implementation must use the
exact approved artifact/config/dictionary fingerprint and must never sort or
silently normalize a dictionary.

~~~text
ppocr/utils/dict/ar_dict.txt
ppocr/utils/dict/arabic_dict.txt
ppocr/utils/dict/be_dict.txt
ppocr/utils/dict/bengali_dict.txt
ppocr/utils/dict/bg_dict.txt
ppocr/utils/dict/bm_dict.txt
ppocr/utils/dict/bm_dict_add.txt
ppocr/utils/dict/bn_dict.txt
ppocr/utils/dict/chinese_cht_dict.txt
ppocr/utils/dict/confuse.pkl
ppocr/utils/dict/cyrillic_dict.txt
ppocr/utils/dict/devanagari_dict.txt
ppocr/utils/dict/en_dict.txt
ppocr/utils/dict/fa_dict.txt
ppocr/utils/dict/french_dict.txt
ppocr/utils/dict/german_dict.txt
ppocr/utils/dict/gujarati_dict.txt
ppocr/utils/dict/hebrew_dict.txt
ppocr/utils/dict/hi_dict.txt
ppocr/utils/dict/it_dict.txt
ppocr/utils/dict/japan_dict.txt
ppocr/utils/dict/ka_dict.txt
ppocr/utils/dict/kazakh_dict.txt
ppocr/utils/dict/korean_dict.txt
ppocr/utils/dict/latex_ocr_tokenizer.json
ppocr/utils/dict/latex_symbol_dict.txt
ppocr/utils/dict/latin_dict.txt
ppocr/utils/dict/mr_dict.txt
ppocr/utils/dict/ne_dict.txt
ppocr/utils/dict/oc_dict.txt
ppocr/utils/dict/parseq_dict.txt
ppocr/utils/dict/ppocrv4_doc_dict.txt
ppocr/utils/dict/ppocrv5_arabic_dict.txt
ppocr/utils/dict/ppocrv5_cyrillic_dict.txt
ppocr/utils/dict/ppocrv5_devanagari_dict.txt
ppocr/utils/dict/ppocrv5_dict.txt
ppocr/utils/dict/ppocrv5_el_dict.txt
ppocr/utils/dict/ppocrv5_en_dict.txt
ppocr/utils/dict/ppocrv5_eslav_dict.txt
ppocr/utils/dict/ppocrv5_korean_dict.txt
ppocr/utils/dict/ppocrv5_latin_dict.txt
ppocr/utils/dict/ppocrv5_ta_dict.txt
ppocr/utils/dict/ppocrv5_te_dict.txt
ppocr/utils/dict/ppocrv5_th_dict.txt
ppocr/utils/dict/ppocrv6_dict.txt
ppocr/utils/dict/ppocrv6_tiny_dict.txt
ppocr/utils/dict/pu_dict.txt
ppocr/utils/dict/rs_dict.txt
ppocr/utils/dict/rsc_dict.txt
ppocr/utils/dict/ru_dict.txt
ppocr/utils/dict/samaritan_dict.txt
ppocr/utils/dict/spin_dict.txt
ppocr/utils/dict/syriac_dict.txt
ppocr/utils/dict/ta_dict.txt
ppocr/utils/dict/table_dict.txt
ppocr/utils/dict/table_master_structure_dict.txt
ppocr/utils/dict/table_structure_dict.txt
ppocr/utils/dict/table_structure_dict_ch.txt
ppocr/utils/dict/te_dict.txt
ppocr/utils/dict/th_dict.txt
ppocr/utils/dict/ug_dict.txt
ppocr/utils/dict/uk_dict.txt
ppocr/utils/dict/ur_dict.txt
ppocr/utils/dict/vi_dict.txt
ppocr/utils/dict/xi_dict.txt
~~~

confuse.pkl must never be loaded through general-purpose unsafe pickle
deserialization in the Rust port. Its exact semantic need must be determined
before any compatible replacement is implemented.

### G.2 Fonts

The baseline includes the following visualization fonts under doc/fonts/:

~~~text
arabic.ttf
chinese_cht.ttf
cyrillic.ttf
french.ttf
german.ttf
hindi.ttf
japan.ttc
kannada.ttf
korean.ttf
latin.ttf
marathi.ttf
nepali.ttf
persian.ttf
simfang.ttf
spanish.ttf
tamil.ttf
telugu.ttf
urdu.ttf
uyghur.ttf
~~~

No font may be copied, embedded, or downloaded by the Rust product until
LIC-001 verifies its independent license and notice obligations. Visualization
is not a prerequisite for a core OCR result contract.

### G.3 Model formats and external artifacts

Observed inference-related artifact conventions include Paddle model parameters
(model or inference.pdiparams plus .pdmodel or .json), optional inference.yml,
and direct ONNX paths. The browser implementation expects a model package
containing inference.onnx and inference.yml plus relevant configuration/dictionary
data.

No model weights are copied into this Rust repository by this inventory. P3 must
choose and validate a model format/runtime and capture source URL, immutable
version, size bounds, SHA-256, artifact/config/dictionary fingerprints, license,
conversion provenance, and supported target matrix in a manifest.

## H. Scope-decision checklist

SCOPE-001 must classify every ID in Sections A-E and each configuration group in
Section C as Must, Should, Later, or Out of scope. It must also answer:

1. Which exact model family/artifacts form the first useful CPU OCR slice?
2. Is compatibility measured against current Python defaults, a compact model,
   a browser ONNX artifact, or another explicit model pair?
3. Which images, PDFs, office documents, URLs, pages, and resource limits are
   supported by the first release?
4. Which public Python behavior is retained as an idiomatic Rust API/CLI/schema
   rather than literal method/argument parity?
5. Which deployment surfaces are native Rust deliverables versus reference-only
   source material, including C++, JavaScript, mobile UI, and Go/TypeScript SDKs?
6. Are training, conversion, export, quantization, and the 155 configuration
   rows part of the first stable release or later milestones?
7. Are local VLM inference, remote VLM adapters, ChatOCR, and translation in
   scope, and which privacy/provider/hardware constraints apply?
8. Which external model, fixture, dictionary, font, and dataset licenses permit
   redistribution?

Until those decisions are approved, this inventory must not be read as
authorization to select a runtime, add an ML dependency, download a model, or
claim PaddleOCR parity.

## I. Inventory evidence

The inventory was produced through read-only inspection of the pinned local
checkout. Recheck the baseline before updating it with:

~~~sh
readlink PaddleOCR
git -C PaddleOCR rev-parse HEAD
git -C PaddleOCR status --short
find PaddleOCR/configs -type f \( -name '*.yml' -o -name '*.yaml' \)
~~~

The last command is an inventory query only. Do not run write-capable commands,
formatters, package installers, tests, or generators inside PaddleOCR/.
