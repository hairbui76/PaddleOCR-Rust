// Copyright 2026 PaddleOCR-Rust Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cooperative cancellation and the wall-clock time policy.
//!
//! Roadmap item `OCR-003` requires a stated cancellation and time policy "where
//! supported". This module is that policy, and the qualifier matters, so it is
//! written down here rather than implied:
//!
//! **A run is abandoned only at a stage boundary.** A backend call, once
//! started, runs to completion. The adapter hands one tensor to the runtime and
//! waits; there is no cancellation channel into it, and inventing one by
//! killing a thread would leave the session in an undefined state. So a request
//! with a one-second budget that reaches a three-second detector call returns
//! after roughly three seconds, not one.
//!
//! That makes the guarantee bounded, not immediate: **overshoot is at most one
//! backend call.** For the classic path that is one detector run, or one
//! recognition batch of at most six crops. A caller that needs a hard wall-clock
//! bound must enforce it out of process; this policy exists to stop a run that
//! would otherwise continue through hundreds of remaining crops, which is the
//! case where the cost is unbounded rather than merely large.
//!
//! Both mechanisms produce a typed error and never a partial result. That is
//! the second half of the policy: see [`crate::pipeline`] for whole-input
//! versus per-item failure semantics.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// How a caller may abandon a run in progress.
///
/// The default imposes no budget and no cancellation, which is what every
/// existing caller gets: an unbudgeted run behaves exactly as it did before this
/// policy existed.
#[derive(Clone, Debug, Default)]
pub struct RunControl {
    /// Wall-clock budget for the whole request, measured from run start.
    time_budget: Option<Duration>,
    /// A flag any thread may set to request that the run stop.
    cancel: Option<Arc<AtomicBool>>,
}

impl RunControl {
    /// Returns a control that never stops a run.
    #[must_use]
    pub fn unbounded() -> Self {
        Self::default()
    }

    /// Sets the wall-clock budget for the whole request.
    ///
    /// The budget starts when the run begins, not when this is called, so one
    /// `RunControl` may be reused across requests without the budget draining.
    #[must_use]
    pub fn with_time_budget(mut self, budget: Duration) -> Self {
        self.time_budget = Some(budget);
        self
    }

    /// Attaches a cancellation flag that any thread may set.
    ///
    /// The flag is read, never written, so one flag may cancel several runs.
    #[must_use]
    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel = Some(flag);
        self
    }

    /// Starts the clock, producing the per-run schedule the pipeline checks.
    pub(crate) fn begin(&self) -> RunSchedule<'_> {
        RunSchedule {
            deadline: self.time_budget.map(|budget| Instant::now() + budget),
            cancel: self.cancel.as_deref(),
        }
    }
}

/// Returns a schedule that never stops a run.
///
/// An unbounded schedule borrows nothing, so it is `'static`. The internal
/// stages' own tests use this rather than constructing a `RunControl` to throw
/// away; the public path always goes through [`RunControl::begin`], because a
/// caller who set a budget must not be able to reach a stage that ignores it.
#[cfg(test)]
pub(crate) fn unbounded_schedule() -> RunSchedule<'static> {
    RunSchedule {
        deadline: None,
        cancel: None,
    }
}

/// One run's resolved deadline and cancellation flag.
pub(crate) struct RunSchedule<'a> {
    /// The instant past which the run must stop, if a budget was set.
    deadline: Option<Instant>,
    /// The caller's cancellation flag, if one was attached.
    cancel: Option<&'a AtomicBool>,
}

impl RunSchedule<'_> {
    /// Returns an error if the run must stop before entering `stage`.
    ///
    /// Cancellation is checked before the deadline: an explicit request from the
    /// caller is more specific than running out of time, and reporting the
    /// timeout instead would misattribute why the run stopped.
    pub(crate) fn check(&self, stage: &'static str) -> Result<()> {
        if let Some(flag) = self.cancel
            && flag.load(Ordering::Relaxed)
        {
            return Err(Error::Cancelled);
        }
        if let Some(deadline) = self.deadline
            && Instant::now() >= deadline
        {
            return Err(Error::TimedOut { stage });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbounded_control_never_stops_a_run() {
        let control = RunControl::unbounded();
        let schedule = control.begin();
        for _ in 0..1_000 {
            assert!(schedule.check("detector").is_ok());
        }
    }

    #[test]
    fn a_set_flag_cancels_at_the_next_stage_boundary() {
        let flag = Arc::new(AtomicBool::new(false));
        let control = RunControl::unbounded().with_cancel_flag(Arc::clone(&flag));
        let schedule = control.begin();
        assert!(schedule.check("detector").is_ok(), "not cancelled yet");

        flag.store(true, Ordering::Relaxed);
        assert!(matches!(
            schedule.check("recognizer.batch"),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn an_exhausted_budget_reports_the_stage_it_stopped_before() {
        // A zero budget is already spent by the time the first check runs.
        let control = RunControl::unbounded().with_time_budget(Duration::ZERO);
        let schedule = control.begin();
        match schedule.check("recognizer.batch") {
            Err(Error::TimedOut { stage }) => assert_eq!(stage, "recognizer.batch"),
            other => panic!("expected a timeout, got {other:?}"),
        }
    }

    /// A budget is spent from run start, so reusing one control across runs does
    /// not hand the second run a shorter budget than the first.
    #[test]
    fn the_budget_restarts_with_every_run() {
        let control = RunControl::unbounded().with_time_budget(Duration::from_secs(3_600));
        assert!(control.begin().check("detector").is_ok());
        assert!(control.begin().check("detector").is_ok());
    }

    /// An explicit cancellation outranks an exhausted budget, because it says
    /// more about why the run stopped.
    #[test]
    fn cancellation_is_reported_ahead_of_a_timeout() {
        let flag = Arc::new(AtomicBool::new(true));
        let control = RunControl::unbounded()
            .with_time_budget(Duration::ZERO)
            .with_cancel_flag(flag);
        let schedule = control.begin();
        assert!(matches!(schedule.check("detector"), Err(Error::Cancelled)));
    }
}
