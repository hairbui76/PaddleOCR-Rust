// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Structured observability that **cannot** leak a document.
//!
//! Roadmap item `OBS-001`. Its dependency is `SERVER-001` **or** `OCR-003`, and
//! `OCR-003` is done, so this is reachable now.
//!
//! # Leaking is prevented by the type, not by discipline
//!
//! The row's requirement is "structured logs/metrics/traces without leaking
//! document text, credentials, URLs, or paths". The usual way to meet that is a
//! rule — *do not log the text* — and a review that enforces it. Rules decay.
//!
//! So [`StageEvent`] has **nowhere to put** a document. Its fields are a
//! `&'static str` stage name, a duration, and counts. There is no `String`, no
//! `PathBuf`, and no generic payload, which means a caller cannot log recognized
//! text through this type even by mistake, and a future field that could carry
//! one would be a visible change to a public struct rather than a slip inside a
//! format string.
//!
//! This is the same argument [`crate::resolve`] makes about offline mode: an
//! absent capability beats a flag that must stay set.
//!
//! # Cardinality is bounded because stage names are `'static`
//!
//! A metrics backend degrades when a label takes unbounded values — one series
//! per document is the classic way to do it. [`Stage`] is a closed enum, so the
//! label set is fixed at compile time and [`STAGE_COUNT`] is its size. A caller
//! cannot introduce a stage named after their file.
//!
//! # What this does not do
//!
//! It does not log. There is no writer, no format, no global, and no
//! dependency: a [`Recorder`] collects events and the caller decides what to do
//! with them. A logging framework is a `SERVER-001` question, and choosing one
//! here would decide it early.
#![allow(dead_code)]

use core::time::Duration;

/// A pipeline stage, and the complete label set for stage metrics.
///
/// Closed on purpose. Adding a variant is a deliberate, reviewable change;
/// accepting a caller-supplied name would let one series become one per input.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Stage {
    /// Decoding an encoded image into pixels.
    Decode,
    /// Optional document orientation and unwarping.
    DocumentPreprocess,
    /// Text detection and its postprocessing.
    Detect,
    /// Perspective cropping of detected regions.
    Crop,
    /// Optional text-line orientation.
    Orientation,
    /// Text recognition and CTC decoding.
    Recognize,
    /// Table classification, cell detection, and structure recognition.
    Table,
}

/// The number of distinct stage labels, and therefore the metric's cardinality.
pub const STAGE_COUNT: usize = 7;

impl Stage {
    /// Every stage, in pipeline order.
    #[must_use]
    pub const fn all() -> [Self; STAGE_COUNT] {
        [
            Self::Decode,
            Self::DocumentPreprocess,
            Self::Detect,
            Self::Crop,
            Self::Orientation,
            Self::Recognize,
            Self::Table,
        ]
    }

    /// A stable label for this stage.
    ///
    /// `&'static str` rather than `String`, so a label can only ever be one of
    /// these.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::DocumentPreprocess => "document_preprocess",
            Self::Detect => "detect",
            Self::Crop => "crop",
            Self::Orientation => "orientation",
            Self::Recognize => "recognize",
            Self::Table => "table",
        }
    }
}

/// One stage's measured work.
///
/// Every field is a number or a `'static` label. There is deliberately no
/// field that can hold document text, a file path, a URL, or a credential —
/// see the module documentation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StageEvent {
    /// Which stage ran.
    pub stage: Stage,
    /// How long it took.
    pub elapsed: Duration,
    /// How many items the stage produced: regions, crops, or lines.
    ///
    /// A count, never the items. Knowing that a page yielded `47` lines is
    /// operationally useful and reveals nothing about what they said.
    pub items: u32,
    /// Whether the stage completed.
    ///
    /// A boolean rather than an error, because an error's payload is exactly
    /// where a path or a message would arrive.
    pub succeeded: bool,
}

/// Collects stage events for one run.
///
/// Bounded: a run has at most one event per stage per invocation, and the
/// recorder caps what it will hold so that an unexpected loop cannot turn
/// observability into a memory leak.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    events: Vec<StageEvent>,
    dropped: usize,
}

/// The largest number of events one recorder retains.
///
/// Generous against a normal run — one per stage — and finite against a bug.
/// Events past the cap are **dropped and counted**, never silently discarded:
/// see [`Recorder::dropped`].
pub const MAX_EVENTS: usize = 1024;

impl Recorder {
    /// A recorder holding nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one stage's result.
    pub fn record(&mut self, event: StageEvent) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        } else {
            self.dropped += 1;
        }
    }

    /// Every recorded event, in the order it happened.
    #[must_use]
    pub fn events(&self) -> &[StageEvent] {
        &self.events
    }

    /// How many events were dropped because the cap was reached.
    ///
    /// Non-zero means the run recorded more stages than [`MAX_EVENTS`], which
    /// is a bug in the caller rather than a normal condition — and saying so is
    /// better than a truncated render that looks complete.
    #[must_use]
    pub const fn dropped(&self) -> usize {
        self.dropped
    }

    /// Total time across every recorded stage.
    #[must_use]
    pub fn total(&self) -> Duration {
        self.events
            .iter()
            .fold(Duration::ZERO, |sum, event| sum + event.elapsed)
    }

    /// Time spent in one stage, summed across its events.
    #[must_use]
    pub fn stage_total(&self, stage: Stage) -> Duration {
        self.events
            .iter()
            .filter(|event| event.stage == stage)
            .fold(Duration::ZERO, |sum, event| sum + event.elapsed)
    }

    /// Renders the run as one line per stage, in a fixed order.
    ///
    /// Deterministic: stages appear in [`Stage::all`] order, not in the order
    /// they were recorded, so two runs of the same shape render identically and
    /// a diff shows a real change rather than a scheduling artefact.
    ///
    /// The output contains only stage labels and numbers. That is the whole
    /// point, and `leaks_nothing_from_a_document` asserts it.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for stage in Stage::all() {
            let events: Vec<&StageEvent> = self
                .events
                .iter()
                .filter(|event| event.stage == stage)
                .collect();
            if events.is_empty() {
                continue;
            }
            let items: u32 = events.iter().map(|event| event.items).sum();
            let failures = events.iter().filter(|event| !event.succeeded).count();
            out.push_str(&format!(
                "stage={} calls={} micros={} items={} failures={}\n",
                stage.label(),
                events.len(),
                self.stage_total(stage).as_micros(),
                items,
                failures
            ));
        }
        out
    }
}

/// Times a closure and records the result.
///
/// Returns whatever the closure returns, so a caller wraps a stage rather than
/// restructuring it. The error itself is **not** recorded — only that the stage
/// failed — because an error's payload is where a path would arrive.
pub fn timed<T, E>(
    recorder: &mut Recorder,
    stage: Stage,
    items: impl FnOnce(&T) -> u32,
    body: impl FnOnce() -> core::result::Result<T, E>,
) -> core::result::Result<T, E> {
    let start = std::time::Instant::now();
    let outcome = body();
    let elapsed = start.elapsed();
    recorder.record(StageEvent {
        stage,
        elapsed,
        items: outcome.as_ref().map_or(0, items),
        succeeded: outcome.is_ok(),
    });
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendered output can carry nothing from a document.
    ///
    /// Not a review of a format string — a check that no arrangement of
    /// recorded events can produce any of these strings, because the type has
    /// nowhere to put them.
    #[test]
    fn leaks_nothing_from_a_document() {
        let secrets = [
            "Hello",
            "你好",
            "/home/user/private/scan.png",
            "https://example.invalid/model",
            "sk-secret-token",
        ];
        let mut recorder = Recorder::new();
        for stage in Stage::all() {
            recorder.record(StageEvent {
                stage,
                elapsed: Duration::from_micros(1_234),
                items: 42,
                succeeded: true,
            });
        }
        let rendered = recorder.render();
        for secret in secrets {
            assert!(
                !rendered.contains(secret),
                "the render leaked {secret:?}: {rendered}"
            );
        }
        // And the only non-numeric tokens are stage labels.
        for line in rendered.lines() {
            for field in line.split(' ') {
                let Some((key, value)) = field.split_once('=') else {
                    panic!("unstructured field {field:?}");
                };
                if key == "stage" {
                    assert!(
                        Stage::all().iter().any(|s| s.label() == value),
                        "unknown stage label {value:?}"
                    );
                } else {
                    assert!(
                        value.parse::<u128>().is_ok(),
                        "field {key} carried a non-number: {value:?}"
                    );
                }
            }
        }
    }

    /// Cardinality is fixed at compile time.
    #[test]
    fn the_label_set_is_closed_and_counted() {
        assert_eq!(Stage::all().len(), STAGE_COUNT);
        let mut labels: Vec<&str> = Stage::all().iter().map(|s| s.label()).collect();
        labels.sort_unstable();
        let before = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), before, "two stages share a label");
    }

    /// Rendering is deterministic and ordered by stage, not by arrival.
    #[test]
    fn the_render_is_deterministic_and_stage_ordered() {
        let mut forward = Recorder::new();
        let mut backward = Recorder::new();
        for stage in Stage::all() {
            forward.record(StageEvent {
                stage,
                elapsed: Duration::from_micros(10),
                items: 1,
                succeeded: true,
            });
        }
        for stage in Stage::all().iter().rev() {
            backward.record(StageEvent {
                stage: *stage,
                elapsed: Duration::from_micros(10),
                items: 1,
                succeeded: true,
            });
        }
        assert_eq!(
            forward.render(),
            backward.render(),
            "arrival order must not change the render"
        );
        assert_eq!(forward.render(), forward.render());

        // First line is the first stage in pipeline order, whichever way the
        // events arrived.
        assert!(
            backward.render().starts_with("stage=decode"),
            "{}",
            backward.render()
        );
    }

    /// A stage that produced nothing is still rendered, with a zero count.
    ///
    /// An absent line and a zero are different facts: "detection did not run"
    /// and "detection found nothing" have different causes.
    #[test]
    fn a_stage_that_found_nothing_is_distinguishable_from_one_that_did_not_run() {
        let mut recorder = Recorder::new();
        recorder.record(StageEvent {
            stage: Stage::Detect,
            elapsed: Duration::from_micros(5),
            items: 0,
            succeeded: true,
        });
        let rendered = recorder.render();
        assert!(rendered.contains("stage=detect"), "{rendered}");
        assert!(rendered.contains("items=0"), "{rendered}");
        assert!(!rendered.contains("stage=recognize"), "{rendered}");
    }

    /// A failing stage records the failure and not the error.
    #[test]
    fn a_failure_records_that_it_failed_and_nothing_else() {
        let mut recorder = Recorder::new();
        let outcome: core::result::Result<u8, &str> = timed(
            &mut recorder,
            Stage::Recognize,
            |value| u32::from(*value),
            || Err("a secret path /home/user/scan.png"),
        );
        assert!(outcome.is_err());
        let rendered = recorder.render();
        assert!(!rendered.contains("/home/user"), "{rendered}");
        assert!(rendered.contains("failures=1"), "{rendered}");
        assert!(rendered.contains("items=0"), "{rendered}");
    }

    /// `timed` returns the value and counts it.
    #[test]
    fn timed_passes_the_value_through_and_counts_it() {
        let mut recorder = Recorder::new();
        let lines: Vec<u8> = match timed(
            &mut recorder,
            Stage::Recognize,
            |value: &Vec<u8>| value.len() as u32,
            || Ok::<Vec<u8>, ()>(vec![1, 2, 3]),
        ) {
            Ok(value) => value,
            Err(()) => panic!("unreachable"),
        };
        assert_eq!(lines, vec![1, 2, 3]);
        assert_eq!(recorder.events().len(), 1);
        assert_eq!(recorder.events()[0].items, 3);
        assert!(recorder.events()[0].succeeded);
    }

    /// The recorder is bounded, so a loop cannot turn it into a leak.
    #[test]
    fn the_recorder_is_bounded() {
        let mut recorder = Recorder::new();
        for _ in 0..(MAX_EVENTS * 2) {
            recorder.record(StageEvent {
                stage: Stage::Crop,
                elapsed: Duration::from_nanos(1),
                items: 1,
                succeeded: true,
            });
        }
        assert_eq!(recorder.events().len(), MAX_EVENTS);
        // And the overflow is **counted**, not silently discarded: a truncated
        // render that looks complete is worse than one that says it is not.
        assert_eq!(recorder.dropped(), MAX_EVENTS);
    }

    /// A run within the cap drops nothing.
    #[test]
    fn a_normal_run_drops_nothing() {
        let mut recorder = Recorder::new();
        for stage in Stage::all() {
            recorder.record(StageEvent {
                stage,
                elapsed: Duration::from_micros(1),
                items: 0,
                succeeded: true,
            });
        }
        assert_eq!(recorder.dropped(), 0);
    }

    /// Overhead: recording a stage costs far less than a stage.
    ///
    /// Not a benchmark. A liveness check that the observability path is not
    /// itself a cost worth measuring — `OBS-001` asks for the overhead, and an
    /// unmeasured "it is cheap" is not an answer.
    #[test]
    fn recording_overhead_is_negligible() {
        let mut recorder = Recorder::new();
        let start = std::time::Instant::now();
        for _ in 0..MAX_EVENTS {
            recorder.record(StageEvent {
                stage: Stage::Detect,
                elapsed: Duration::from_nanos(1),
                items: 1,
                succeeded: true,
            });
        }
        let elapsed = start.elapsed();
        // A detector run on one page is tens of milliseconds; 1,024 recordings
        // must be orders below that.
        assert!(
            elapsed < Duration::from_millis(50),
            "{MAX_EVENTS} recordings took {elapsed:?}"
        );
        assert!(!recorder.render().is_empty());
    }
}
