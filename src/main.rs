#![forbid(unsafe_code)]
// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Command-line entrypoint for the classic OCR path.
//!
//! Every artifact is supplied explicitly: nothing is downloaded, cached, or
//! read from an environment variable. Without the `onnxruntime` feature there
//! is no backend compiled in, so the binary reports that and exits rather than
//! pretending to work.

use std::process::ExitCode;

fn usage() -> &'static str {
    "usage: paddleocr-rust --ort-dylib <libonnxruntime.so> \\\n\
     \x20                 --detector <detector.onnx> --recognizer <recognizer.onnx> \\\n\
     \x20                 --dictionary <dict.txt> [--json] [--time-budget-ms <n>] \\\n\
     \x20                 [--manifest <manifest.txt>] \\\n\
     \x20                 [--orientation <textline_ori.onnx> [--orientation-sha256 <hex>]] \\\n\
     \x20                 [--detector-sha256 <hex>] [--recognizer-sha256 <hex>] <image.png>...\n\
     \n\
     All paths are explicit. PNG and JPEG input are supported.\n\
     \n\
     Several images may be given. The models are loaded once and reused, which \n\
     is the whole reason to pass them together rather than running this per \n\
     file. With more than one image the text output gains a leading path \n\
     column, as grep does, and --json emits one JSON document per line.\n\
     \n\
     Three further commands take the same model flags:\n\
     \n\
     \x20 paddleocr-rust structure --layout <layout.onnx> \\\n\
     \x20     [--table-classifier <cls.onnx> --table-cells <cell.onnx> \\\n\
     \x20      --table-structure <str.onnx> [--route wired|wireless]] \\\n\
     \x20     [--format markdown|json|text] [--plain] [--id <text>] <page.png>\n\
     \n\
     \x20 paddleocr-rust table --table-classifier <cls.onnx> \\\n\
     \x20     --table-cells <cell.onnx> --table-structure <str.onnx> \\\n\
     \x20     [--route wired|wireless] [--format json|html] [--id <text>] <crop.png>\n\
     \n\
     \x20 paddleocr-rust pdf [--json] [--first-page <n>] [--pages <n>] \\\n\
     \x20     <document.pdf>\n\
     \n\
     structure parses a page into ordered blocks and Markdown; the three table \n\
     flags are all-or-none and turn table recognition on. table recognizes one \n\
     crop, using the crop's own OCR to fill the cells. Both take exactly one \n\
     image.\n\
     \n\
     pdf reads a whole document, one result per page, and needs a build with \n\
     --features onnxruntime,pdf. A page that cannot be read is reported on \n\
     stderr and does not stop the run; the exit code is 1 if any page failed. \n\
     Run `paddleocr-rust pdf --help` for its own options.\n"
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    // A subcommand documents its own options, so `pdf --help` must reach `pdf`
    // rather than printing the top-level usage over it. Only a --help with no
    // subcommand in front of it is answered here.
    let leads_with_subcommand = arguments
        .first()
        .is_some_and(|first| matches!(first.as_str(), "structure" | "table" | "pdf"));
    if !leads_with_subcommand
        && arguments
            .iter()
            .any(|value| value == "--help" || value == "-h")
    {
        print!("{}", usage());
        return ExitCode::SUCCESS;
    }

    // A leading `structure` or `table` selects the newer commands; anything
    // else is the classic path this binary has always run, unchanged. The
    // check is on the first argument only, so a *file* named `table` still
    // reaches the classic path when it is not first — and a caller who does
    // lead with such a file gets a message rather than silence.
    let subcommand = arguments
        .first()
        .map(String::as_str)
        .filter(|first| matches!(*first, "structure" | "table" | "pdf"));

    #[cfg(not(feature = "onnxruntime"))]
    {
        let _ = (&arguments, subcommand);
        eprintln!(
            "paddleocr-rust: this build has no inference backend compiled in.\n\
             Rebuild with `--features onnxruntime` to run the classic pipeline."
        );
        eprint!("\n{}", usage());
        ExitCode::from(2)
    }

    #[cfg(feature = "onnxruntime")]
    {
        let outcome = match subcommand {
            // A build without the `pdf` feature must say so rather than treat
            // the word as a filename and fail with a confusing decode error.
            Some("pdf") => {
                #[cfg(feature = "pdf")]
                {
                    documents::run(&arguments[1..])
                }
                #[cfg(not(feature = "pdf"))]
                {
                    Err("this build has no PDF support compiled in; rebuild with \
                         `--features onnxruntime,pdf`"
                        .to_owned())
                }
            }
            Some(name) => structured::run(name, &arguments[1..]),
            None => run(&arguments),
        };
        match outcome {
            Ok(code) => code,
            Err(message) => {
                eprintln!("paddleocr-rust: {message}");
                ExitCode::from(2)
            }
        }
    }
}

#[cfg(feature = "onnxruntime")]
fn run(arguments: &[String]) -> Result<ExitCode, String> {
    let mut library = None;
    let mut detector = None;
    let mut recognizer = None;
    let mut dictionary = None;
    let mut images: Vec<String> = Vec::new();
    let mut json = false;
    let mut time_budget_ms = None;
    let mut manifest_path = None;
    let mut orientation = None;
    let mut orientation_sha256 = None;
    let mut detector_sha256 = None;
    let mut recognizer_sha256 = None;

    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        let take = |slot: &mut Option<String>| -> Result<(), String> {
            let value = arguments
                .get(index + 1)
                .ok_or_else(|| format!("{argument} needs a value"))?;
            *slot = Some(value.clone());
            Ok(())
        };
        match argument {
            "--ort-dylib" => {
                take(&mut library)?;
                index += 2;
            }
            "--detector" => {
                take(&mut detector)?;
                index += 2;
            }
            "--recognizer" => {
                take(&mut recognizer)?;
                index += 2;
            }
            "--dictionary" => {
                take(&mut dictionary)?;
                index += 2;
            }
            "--detector-sha256" => {
                take(&mut detector_sha256)?;
                index += 2;
            }
            "--recognizer-sha256" => {
                take(&mut recognizer_sha256)?;
                index += 2;
            }
            "--time-budget-ms" => {
                take(&mut time_budget_ms)?;
                index += 2;
            }
            "--manifest" => {
                take(&mut manifest_path)?;
                index += 2;
            }
            "--orientation" => {
                take(&mut orientation)?;
                index += 2;
            }
            "--orientation-sha256" => {
                take(&mut orientation_sha256)?;
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}"));
            }
            other => {
                images.push(other.to_owned());
                index += 1;
            }
        }
    }

    let (library, detector, recognizer, dictionary) =
        match (library, detector, recognizer, dictionary) {
            (Some(a), Some(b), Some(c), Some(d)) if !images.is_empty() => (a, b, c, d),
            _ => return Err(format!("missing required arguments\n\n{}", usage())),
        };

    // The budget is checked at stage boundaries, so it bounds a run rather than
    // interrupting one; see the `control` module for what that guarantees. It
    // applies per image, not to the whole invocation: a caller passing fifty
    // pages means "no page may take longer than this", and a total budget would
    // silently abandon the tail of the list.
    let mut options = paddleocr_rust::api::OcrOptions::default();
    if let Some(value) = time_budget_ms {
        let milliseconds: u64 = value
            .parse()
            .map_err(|_| format!("--time-budget-ms needs a whole number, got {value:?}"))?;
        options.control = paddleocr_rust::control::RunControl::unbounded()
            .with_time_budget(std::time::Duration::from_millis(milliseconds));
    }

    // A manifest is provenance and identity, never a download instruction and
    // never a path resolver: the artifact paths above are still the caller's.
    // Its digests are applied as the expected ones when the caller did not give
    // them explicitly, so declaring a manifest is also declaring verification.
    let manifest = match &manifest_path {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read the manifest: {error}"))?;
            let parsed = paddleocr_rust::manifest::ModelManifest::parse(&text)
                .map_err(|error| format!("{path}: {error}"))?;
            eprintln!(
                "manifest: {} {} ({} via {})",
                parsed.family, parsed.version, parsed.format, parsed.backend
            );
            Some(parsed)
        }
        None => None,
    };
    if let Some(parsed) = &manifest {
        if detector_sha256.is_none() {
            detector_sha256 = Some(parsed.detector.sha256.clone());
        }
        if recognizer_sha256.is_none() {
            recognizer_sha256 = Some(parsed.recognizer.sha256.clone());
        }
    }

    let dictionary_text = std::fs::read_to_string(&dictionary)
        .map_err(|error| format!("cannot read the dictionary: {error}"))?;
    let parsed = paddleocr_rust::api::parse_dictionary(&dictionary_text, true)
        .map_err(|error| format!("{error}"))?;
    eprintln!("dictionary: {} entries", parsed.len());
    // A manifest that disagrees with the dictionary it was given describes a
    // different pairing than the one about to run, so it is refused rather than
    // recorded alongside a result it does not describe.
    if let Some(expected) = &manifest
        && expected.dictionary.entries != parsed.len()
    {
        return Err(format!(
            "the manifest declares {} dictionary entries but the file has {}",
            expected.dictionary.entries,
            parsed.len()
        ));
    }

    // Loading once is the point of the engine: session creation costs about
    // 1.4 s on the reference host, and paying it per image would make a
    // multi-page run several times slower than it needs to be.
    let engine = paddleocr_rust::api::OcrEngine::load(
        &{
            let mut artifacts =
                paddleocr_rust::api::Artifacts::new(&library, &detector, &recognizer);
            if let Some(digest) = detector_sha256.as_deref() {
                artifacts = artifacts.with_detector_sha256(digest);
            }
            if let Some(digest) = recognizer_sha256.as_deref() {
                artifacts = artifacts.with_recognizer_sha256(digest);
            }
            if let Some(path) = orientation.as_deref() {
                artifacts = artifacts.with_orientation(path);
                eprintln!("orientation: {path}");
            }
            if let Some(digest) = orientation_sha256.as_deref() {
                artifacts = artifacts.with_orientation_sha256(digest);
            }
            artifacts
        },
        &parsed,
    )
    .map_err(|error| format!("{error}"))?;

    let several = images.len() > 1;
    for image in &images {
        // Bounded during the read: `std::fs::read` would allocate a ten
        // gigabyte file in full and only then meet the 64 MiB limit, which
        // honours the limit's letter and defeats its purpose.
        let bytes = paddleocr_rust::input::read_encoded_file(image)
            .map_err(|error| format!("{image}: {error}"))?;
        let (width, height) = paddleocr_rust::api::decode_image(&bytes)
            .map_err(|error| format!("{image}: {error}"))?;
        eprintln!("image: {image} ({width}x{height} PNG)");

        let lines = engine
            .recognize_image(&bytes, &options)
            .map_err(|error| format!("{image}: {error}"))?;

        if json {
            // One document per line: with several inputs that is JSONL, and
            // each document names its input so position is not the only thing
            // identifying it.
            let id = if several { Some(image.as_str()) } else { None };
            println!(
                "{}",
                paddleocr_rust::result_json::result_to_json(
                    &lines,
                    width,
                    height,
                    id,
                    manifest.as_ref(),
                )
            );
        } else {
            for line in &lines {
                if several {
                    println!("{image}\t{:.6}\t{}", line.score, line.text);
                } else {
                    println!("{:.6}\t{}", line.score, line.text);
                }
            }
            if lines.is_empty() {
                eprintln!("{image}: no text detected");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The `structure` and `table` commands.
///
/// Kept apart from the classic path above rather than folded into it: that
/// path has a stable, documented invocation with no subcommand, and threading
/// three more optional models through its flag loop would put every new
/// failure mode in front of callers who never asked for one. The parsing here
/// is pure and unit-tested; only the running needs a backend.
///
/// Without the inference feature nothing but the tests calls the parser, so
/// the allowance is scoped to that build rather than blanket: with a backend
/// compiled in, genuinely unreachable code here still fails the lint.
#[cfg_attr(not(feature = "onnxruntime"), allow(dead_code))]
mod structured {
    /// Which command was asked for.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Kind {
        /// Parse a page into ordered blocks and Markdown.
        Structure,
        /// Recognize one table crop.
        Table,
    }

    /// How the result is written.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Format {
        /// The frozen JSON document.
        Json,
        /// The page's Markdown.
        Markdown,
        /// Block contents, one per line.
        Text,
        /// The table's assembled HTML.
        Html,
    }

    /// One parsed invocation.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Request {
        pub kind: Kind,
        pub library: String,
        pub detector: String,
        pub recognizer: String,
        pub dictionary: String,
        pub orientation: Option<String>,
        pub detector_sha256: Option<String>,
        pub recognizer_sha256: Option<String>,
        pub orientation_sha256: Option<String>,
        pub layout: Option<String>,
        pub table: Option<[String; 3]>,
        pub wired: bool,
        pub format: Format,
        pub pretty: bool,
        pub id: Option<String>,
        pub time_budget_ms: Option<u64>,
        pub image: String,
    }

    fn take(
        slot: &mut Option<String>,
        flag: &str,
        arguments: &[String],
        index: usize,
    ) -> Result<(), String> {
        if slot.is_some() {
            return Err(format!("{flag} was given more than once"));
        }
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        *slot = Some(value.clone());
        Ok(())
    }

    /// Parses one `structure` or `table` invocation, excluding the command word.
    pub fn parse(name: &str, arguments: &[String]) -> Result<Request, String> {
        let kind = match name {
            "structure" => Kind::Structure,
            "table" => Kind::Table,
            other => return Err(format!("unknown command {other}")),
        };

        let mut library = None;
        let mut detector = None;
        let mut recognizer = None;
        let mut dictionary = None;
        let mut orientation = None;
        let mut detector_sha256 = None;
        let mut recognizer_sha256 = None;
        let mut orientation_sha256 = None;
        let mut layout = None;
        let mut classifier = None;
        let mut cells = None;
        let mut structure = None;
        let mut route = None;
        let mut format = None;
        let mut identifier = None;
        let mut time_budget = None;
        let mut plain = false;
        let mut image: Option<String> = None;

        let mut index = 0;
        while index < arguments.len() {
            let argument = arguments[index].as_str();
            if argument == "--plain" {
                if plain {
                    return Err("--plain was given more than once".to_owned());
                }
                plain = true;
                index += 1;
                continue;
            }
            let slot = match argument {
                "--ort-dylib" => &mut library,
                "--detector" => &mut detector,
                "--recognizer" => &mut recognizer,
                "--dictionary" => &mut dictionary,
                "--orientation" => &mut orientation,
                "--detector-sha256" => &mut detector_sha256,
                "--recognizer-sha256" => &mut recognizer_sha256,
                "--orientation-sha256" => &mut orientation_sha256,
                "--layout" => &mut layout,
                "--table-classifier" => &mut classifier,
                "--table-cells" => &mut cells,
                "--table-structure" => &mut structure,
                "--route" => &mut route,
                "--format" => &mut format,
                "--id" => &mut identifier,
                "--time-budget-ms" => &mut time_budget,
                other if other.starts_with("--") => {
                    return Err(format!("unknown option {other}"));
                }
                other => {
                    if image.is_some() {
                        return Err(format!(
                            "{name} takes exactly one image, and {other} is a second"
                        ));
                    }
                    image = Some(other.to_owned());
                    index += 1;
                    continue;
                }
            };
            take(slot, argument, arguments, index)?;
            index += 2;
        }

        let required = |slot: Option<String>, flag: &str| -> Result<String, String> {
            slot.ok_or_else(|| format!("{flag} is required"))
        };
        let image = image.ok_or_else(|| format!("{name} needs one image"))?;

        let wired = match route.as_deref() {
            None | Some("wired") => true,
            Some("wireless") => false,
            Some(other) => return Err(format!("--route must be wired or wireless, got {other}")),
        };
        let table = match (classifier, cells, structure) {
            (Some(classifier), Some(cells), Some(structure)) => {
                Some([classifier, cells, structure])
            }
            (None, None, None) => None,
            _ => {
                return Err(
                    "--table-classifier, --table-cells, and --table-structure must be given \
                     together"
                        .to_owned(),
                );
            }
        };
        let time_budget_ms =
            match time_budget {
                Some(value) => Some(value.parse::<u64>().map_err(|_| {
                    format!("--time-budget-ms needs a whole number, got {value:?}")
                })?),
                None => None,
            };

        let format = match (kind, format.as_deref()) {
            (Kind::Structure, None | Some("markdown")) => Format::Markdown,
            (Kind::Structure, Some("json")) => Format::Json,
            (Kind::Structure, Some("text")) => Format::Text,
            (Kind::Structure, Some(other)) => {
                return Err(format!(
                    "structure --format must be markdown, json, or text, got {other}"
                ));
            }
            (Kind::Table, None | Some("json")) => Format::Json,
            (Kind::Table, Some("html")) => Format::Html,
            (Kind::Table, Some(other)) => {
                return Err(format!("table --format must be json or html, got {other}"));
            }
        };

        let layout = match kind {
            Kind::Structure => Some(required(layout, "--layout")?),
            Kind::Table => {
                if layout.is_some() {
                    return Err("--layout does not apply to table".to_owned());
                }
                None
            }
        };
        if kind == Kind::Table && table.is_none() {
            return Err(
                "table needs --table-classifier, --table-cells, and --table-structure".to_owned(),
            );
        }
        if kind == Kind::Table && plain {
            return Err("--plain does not apply to table".to_owned());
        }

        Ok(Request {
            kind,
            library: required(library, "--ort-dylib")?,
            detector: required(detector, "--detector")?,
            recognizer: required(recognizer, "--recognizer")?,
            dictionary: required(dictionary, "--dictionary")?,
            orientation,
            detector_sha256,
            recognizer_sha256,
            orientation_sha256,
            layout,
            table,
            wired,
            format,
            pretty: !plain,
            id: identifier,
            time_budget_ms,
            image,
        })
    }

    #[cfg(feature = "onnxruntime")]
    pub use execute::run;

    #[cfg(feature = "onnxruntime")]
    mod execute {
        use std::process::ExitCode;

        use paddleocr_rust::api::{Artifacts, OcrEngine, OcrOptions};
        use paddleocr_rust::structure_engine::{
            StructureArtifacts, StructureEngine, StructureOptions,
        };
        use paddleocr_rust::table_engine::{TableArtifacts, TableEngine, TableImage};
        use paddleocr_rust::table_pipeline::TableRoute;

        use super::{Format, Kind, Request, parse};

        /// Parses and runs one `structure` or `table` invocation.
        pub fn run(name: &str, arguments: &[String]) -> Result<ExitCode, String> {
            let request = parse(name, arguments)?;
            let dictionary_text = std::fs::read_to_string(&request.dictionary)
                .map_err(|error| format!("cannot read the dictionary: {error}"))?;
            let dictionary = paddleocr_rust::api::parse_dictionary(&dictionary_text, true)
                .map_err(|error| format!("{error}"))?;
            eprintln!("dictionary: {} entries", dictionary.len());

            let bytes = paddleocr_rust::input::read_encoded_file(&request.image)
                .map_err(|error| format!("{}: {error}", request.image))?;
            let (width, height) = paddleocr_rust::api::decode_image(&bytes)
                .map_err(|error| format!("{}: {error}", request.image))?;
            eprintln!("image: {} ({width}x{height})", request.image);

            let mut options = OcrOptions::default();
            if let Some(milliseconds) = request.time_budget_ms {
                options.control = paddleocr_rust::control::RunControl::unbounded()
                    .with_time_budget(std::time::Duration::from_millis(milliseconds));
            }

            let output = match request.kind {
                Kind::Structure => run_structure(&request, &dictionary, &bytes, options)?,
                Kind::Table => run_table(&request, &dictionary, &bytes, options)?,
            };
            println!("{output}");
            Ok(ExitCode::SUCCESS)
        }

        fn artifacts_of(request: &Request) -> Artifacts<'_> {
            let mut artifacts =
                Artifacts::new(&request.library, &request.detector, &request.recognizer);
            if let Some(digest) = request.detector_sha256.as_deref() {
                artifacts = artifacts.with_detector_sha256(digest);
            }
            if let Some(digest) = request.recognizer_sha256.as_deref() {
                artifacts = artifacts.with_recognizer_sha256(digest);
            }
            if let Some(path) = request.orientation.as_deref() {
                artifacts = artifacts.with_orientation(path);
            }
            if let Some(digest) = request.orientation_sha256.as_deref() {
                artifacts = artifacts.with_orientation_sha256(digest);
            }
            artifacts
        }

        fn table_artifacts<'a>(request: &'a Request, table: &'a [String; 3]) -> TableArtifacts<'a> {
            TableArtifacts::new(
                &request.library,
                &table[0],
                &table[1],
                &table[2],
                if request.wired {
                    TableRoute::Wired
                } else {
                    TableRoute::Wireless
                },
            )
        }

        fn run_structure(
            request: &Request,
            dictionary: &paddleocr_rust::api::Dictionary,
            bytes: &[u8],
            options: OcrOptions,
        ) -> Result<String, String> {
            let layout = request
                .layout
                .as_deref()
                .ok_or_else(|| "--layout is required".to_owned())?;
            let mut artifacts = StructureArtifacts::new(artifacts_of(request), layout);
            if let Some(table) = &request.table {
                artifacts = artifacts.with_table(table_artifacts(request, table));
            }
            let engine = StructureEngine::load(&artifacts, dictionary)
                .map_err(|error| format!("cannot load the structure models: {error}"))?;

            let mut structure_options = StructureOptions::new(options);
            structure_options.pretty = request.pretty;
            let result = engine
                .parse_image(bytes, &structure_options)
                .map_err(|error| format!("{}: {error}", request.image))?;
            eprintln!("blocks: {}", result.blocks.len());

            Ok(match request.format {
                Format::Json => result.to_json(request.id.as_deref()),
                Format::Text => result
                    .blocks
                    .iter()
                    .map(|block| block.content.as_str())
                    .filter(|content| !content.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => result.markdown,
            })
        }

        fn run_table(
            request: &Request,
            dictionary: &paddleocr_rust::api::Dictionary,
            bytes: &[u8],
            options: OcrOptions,
        ) -> Result<String, String> {
            let table = request
                .table
                .as_ref()
                .ok_or_else(|| "the table models are required".to_owned())?;

            // The crop's own OCR fills the cells: a table recognized without
            // text would be a structure of empty cells, which reads as "these
            // cells are blank" rather than "recognition did not run".
            let ocr = OcrEngine::load(&artifacts_of(request), dictionary)
                .map_err(|error| format!("cannot load the OCR models: {error}"))?;
            let lines = ocr
                .recognize_image(bytes, &options)
                .map_err(|error| format!("{}: {error}", request.image))?;

            let bgr =
                TableImage::decode(bytes).map_err(|error| format!("{}: {error}", request.image))?;
            let rgb = bgr
                .with_swapped_channels()
                .map_err(|error| format!("{error}"))?;
            let dimensions = bgr.dimensions();
            let (width, height) = (dimensions.width(), dimensions.height());

            let mut boxes = Vec::with_capacity(lines.len());
            let mut texts = Vec::with_capacity(lines.len());
            for line in &lines {
                let mut bbox = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for point in line.quadrilateral.points() {
                    bbox[0] = bbox[0].min(f64::from(point.x()));
                    bbox[1] = bbox[1].min(f64::from(point.y()));
                    bbox[2] = bbox[2].max(f64::from(point.x()));
                    bbox[3] = bbox[3].max(f64::from(point.y()));
                }
                boxes.push(bbox);
                texts.push(line.text.clone());
            }

            let engine = TableEngine::load(&table_artifacts(request, table))
                .map_err(|error| format!("cannot load the table models: {error}"))?;
            let result = engine
                .recognize_table(
                    &rgb,
                    &bgr,
                    [0.0, 0.0, f64::from(width), f64::from(height)],
                    &boxes,
                    &texts,
                )
                .map_err(|error| format!("{}: {error}", request.image))?;

            Ok(match request.format {
                Format::Html => result.html,
                _ => result.to_json(width, height, request.id.as_deref()),
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn parse_words(name: &str, line: &str) -> Result<Request, String> {
            let arguments: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            parse(name, &arguments)
        }

        const MODELS: &str =
            "--ort-dylib lib.so --detector det.onnx --recognizer rec.onnx --dictionary dict.txt";

        #[test]
        fn structure_collects_its_models_and_defaults_to_pretty_markdown() {
            let request = match parse_words("structure", &format!("{MODELS} --layout l.onnx p.png"))
            {
                Ok(request) => request,
                Err(error) => panic!("{error}"),
            };
            assert_eq!(request.kind, Kind::Structure);
            assert_eq!(request.library, "lib.so");
            assert_eq!(request.layout.as_deref(), Some("l.onnx"));
            assert_eq!(request.image, "p.png");
            assert_eq!(request.format, Format::Markdown);
            assert!(request.pretty);
            assert_eq!(request.table, None);
        }

        /// The flag vocabulary is the classic path's, not a second one.
        #[test]
        fn the_library_flag_keeps_its_existing_name() {
            let error = match parse_words(
                "structure",
                "--library lib.so --detector d --recognizer r --dictionary x --layout l p.png",
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("--library"), "{error}");
        }

        #[test]
        fn a_missing_or_repeated_flag_names_itself() {
            let error = match parse_words("structure", &format!("{MODELS} p.png")) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("--layout"), "{error}");

            let error = match parse_words(
                "structure",
                &format!("{MODELS} --layout a.onnx --layout b.onnx p.png"),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("more than once"), "{error}");
        }

        #[test]
        fn the_table_trio_is_all_or_none() {
            let error = match parse_words(
                "structure",
                &format!("{MODELS} --layout l.onnx --table-cells c.onnx p.png"),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("together"), "{error}");

            let request = match parse_words(
                "structure",
                &format!(
                    "{MODELS} --layout l.onnx --table-classifier a --table-cells b \
                     --table-structure c p.png"
                ),
            ) {
                Ok(request) => request,
                Err(error) => panic!("{error}"),
            };
            assert_eq!(
                request.table,
                Some(["a".to_owned(), "b".to_owned(), "c".to_owned()])
            );
            assert!(request.wired, "the default route is wired");
        }

        #[test]
        fn table_requires_its_trio_and_refuses_layout() {
            let error = match parse_words("table", &format!("{MODELS} crop.png")) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("--table-classifier"), "{error}");

            let error = match parse_words(
                "table",
                &format!(
                    "{MODELS} --layout l.onnx --table-classifier a --table-cells b \
                     --table-structure c crop.png"
                ),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("--layout"), "{error}");
        }

        /// One image, because joining pages needs the renderer gate.
        #[test]
        fn exactly_one_image_is_accepted() {
            let error = match parse_words(
                "structure",
                &format!("{MODELS} --layout l.onnx a.png b.png"),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("exactly one image"), "{error}");

            let error = match parse_words("structure", &format!("{MODELS} --layout l.onnx")) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("needs one image"), "{error}");
        }

        #[test]
        fn formats_and_routes_are_validated_per_command() {
            let error = match parse_words(
                "structure",
                &format!("{MODELS} --layout l.onnx --format html p.png"),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("markdown"), "{error}");

            let error = match parse_words(
                "table",
                &format!(
                    "{MODELS} --table-classifier a --table-cells b --table-structure c \
                     --format markdown crop.png"
                ),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("json or html"), "{error}");

            let error = match parse_words(
                "table",
                &format!(
                    "{MODELS} --table-classifier a --table-cells b --table-structure c \
                     --route sideways crop.png"
                ),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("wired or wireless"), "{error}");
        }

        #[test]
        fn the_time_budget_and_digests_carry_over_from_the_classic_path() {
            let request = match parse_words(
                "structure",
                &format!(
                    "{MODELS} --layout l.onnx --time-budget-ms 5000 --detector-sha256 abc \
                     --orientation o.onnx --id page p.png"
                ),
            ) {
                Ok(request) => request,
                Err(error) => panic!("{error}"),
            };
            assert_eq!(request.time_budget_ms, Some(5_000));
            assert_eq!(request.detector_sha256.as_deref(), Some("abc"));
            assert_eq!(request.orientation.as_deref(), Some("o.onnx"));
            assert_eq!(request.id.as_deref(), Some("page"));

            let error = match parse_words(
                "structure",
                &format!("{MODELS} --layout l.onnx --time-budget-ms soon p.png"),
            ) {
                Err(error) => error,
                Ok(request) => panic!("expected a failure, got {request:?}"),
            };
            assert!(error.contains("whole number"), "{error}");
        }
    }
}

/// The `pdf` command.
///
/// Deliberately closer to the classic path than to `structure`: it needs the
/// same detector, recognizer, and dictionary and nothing else, so it reuses that
/// flag vocabulary rather than inventing one.
///
/// Output reuses the **frozen** `paddleocr-rust/ocr-result/v1` document, one per
/// page, exactly as the classic path already does for several images. A
/// per-document PDF schema would need its own contract under `API-DEC-001`, and
/// minting an unfrozen one to save a wrapper would be the wrong trade.
///
/// A failed page goes to stderr with its page index and does **not** stop the
/// run, which is the recorded per-page policy surfaced at the command line. The
/// exit code is `1` when any page failed and `0` when none did, so a script can
/// tell a fully-read document from a partly-read one without parsing anything.
#[cfg(all(feature = "onnxruntime", feature = "pdf"))]
mod documents {
    use std::process::ExitCode;

    use paddleocr_rust::api::{Artifacts, OcrEngine, OcrOptions, PdfPageRange, parse_dictionary};

    pub fn usage() -> &'static str {
        "usage: paddleocr-rust pdf --ort-dylib <libonnxruntime.so> \\\n\
         \x20        --detector <detector.onnx> --recognizer <recognizer.onnx> \\\n\
         \x20        --dictionary <dict.txt> [--json] [--time-budget-ms <n>] \\\n\
         \x20        [--first-page <n>] [--pages <n>] <document.pdf>\n\
         \n\
         Pages are numbered from 1 on the command line and reported the same way.\n\
         One `ocr-result/v1` document per page with --json. A page that cannot be\n\
         read is reported on stderr and does not stop the run; the exit code is 1\n\
         if any page failed.\n"
    }

    pub fn run(arguments: &[String]) -> Result<ExitCode, String> {
        let mut library = None;
        let mut detector = None;
        let mut recognizer = None;
        let mut dictionary = None;
        let mut document = None;
        let mut json = false;
        let mut time_budget = None;
        let mut first_page = None;
        let mut pages = None;

        let mut rest = arguments.iter();
        while let Some(argument) = rest.next() {
            let mut value = |name: &str| -> Result<String, String> {
                rest.next()
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value"))
            };
            match argument.as_str() {
                "--ort-dylib" => library = Some(value("--ort-dylib")?),
                "--detector" => detector = Some(value("--detector")?),
                "--recognizer" => recognizer = Some(value("--recognizer")?),
                "--dictionary" => dictionary = Some(value("--dictionary")?),
                "--time-budget-ms" => time_budget = Some(value("--time-budget-ms")?),
                "--first-page" => first_page = Some(value("--first-page")?),
                "--pages" => pages = Some(value("--pages")?),
                "--json" => json = true,
                "--help" | "-h" => {
                    print!("{}", usage());
                    return Ok(ExitCode::SUCCESS);
                }
                other if other.starts_with("--") => {
                    return Err(format!("unknown option {other}"));
                }
                other => {
                    if document.replace(other.to_owned()).is_some() {
                        return Err("pdf takes exactly one document".to_owned());
                    }
                }
            }
        }

        let (Some(library), Some(detector), Some(recognizer), Some(dictionary), Some(document)) =
            (library, detector, recognizer, dictionary, document)
        else {
            return Err(format!(
                "pdf needs --ort-dylib, --detector, --recognizer, --dictionary, and one document\n\n{}",
                usage()
            ));
        };

        // One-based on the command line, zero-based inside: a page number a
        // human types should match the one their viewer shows.
        let first = match first_page.as_deref() {
            Some(value) => match value.parse::<u32>() {
                Ok(0) | Err(_) => return Err("--first-page must be 1 or greater".to_owned()),
                Ok(number) => number - 1,
            },
            None => 0,
        };
        let range = match pages.as_deref() {
            Some(value) => match value.parse::<u32>() {
                Ok(0) | Err(_) => return Err("--pages must be 1 or greater".to_owned()),
                Ok(count) => PdfPageRange::from(first, count),
            },
            None if first == 0 => PdfPageRange::all(),
            None => PdfPageRange::from(first, u32::MAX),
        };

        let text = std::fs::read_to_string(&dictionary)
            .map_err(|error| format!("{dictionary}: {error}"))?;
        let parsed = parse_dictionary(&text, true).map_err(|error| format!("{error}"))?;
        eprintln!("dictionary: {} entries", parsed.len());

        let mut options = OcrOptions::default();
        if let Some(value) = time_budget.as_deref() {
            let millis = value
                .parse::<u64>()
                .map_err(|_| "--time-budget-ms must be a whole number".to_owned())?;
            options = options.with_control(
                paddleocr_rust::control::RunControl::unbounded()
                    .with_time_budget(std::time::Duration::from_millis(millis)),
            );
        }

        let engine = OcrEngine::load(&Artifacts::new(&library, &detector, &recognizer), &parsed)
            .map_err(|error| format!("{error}"))?;

        // Bounded during the read, like every other input this binary takes.
        let bytes = paddleocr_rust::input::read_encoded_file(&document)
            .map_err(|error| format!("{document}: {error}"))?;
        let result = engine
            .recognize_pdf(&bytes, range, &options)
            .map_err(|error| format!("{document}: {error}"))?;
        eprintln!(
            "document: {document} ({} pages, {} selected)",
            result.page_count,
            result.pages.len()
        );

        for page in &result.pages {
            let number = page.index + 1;
            match &page.outcome {
                Ok(parsed_page) => {
                    if json {
                        let id = format!("{document}#page={number}");
                        println!(
                            "{}",
                            paddleocr_rust::result_json::result_to_json(
                                &parsed_page.lines,
                                parsed_page.width_pixels,
                                parsed_page.height_pixels,
                                Some(&id),
                                None,
                            )
                        );
                    } else {
                        for line in &parsed_page.lines {
                            println!("{number}\t{:.6}\t{}", line.score, line.text);
                        }
                        if parsed_page.lines.is_empty() {
                            eprintln!("page {number}: no text detected");
                        }
                    }
                }
                // Reported, never swallowed, and never fatal to the rest.
                Err(error) => eprintln!("page {number}: {error}"),
            }
        }

        if result.failed() > 0 {
            eprintln!(
                "{} of {} selected page(s) could not be read",
                result.failed(),
                result.pages.len()
            );
            return Ok(ExitCode::from(1));
        }
        Ok(ExitCode::SUCCESS)
    }
}
