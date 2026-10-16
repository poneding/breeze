//! One active parse and one newest pending snapshot, with stale results rejected.
//!
//! Parsing and projecting a whole document is bounded work, but it is not
//! *instant* work: a megabyte of prose takes long enough that doing it per
//! keystroke would show up in the typing latency. The editor therefore paints a
//! provisional projection immediately and asks this pipeline for the refined
//! one. Nothing here touches the UI: the coordinator is a plain state machine so
//! the admission policy can be tested without a window.
use super::{EditSession, Projection, ProjectionMode, Semantic};
use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

/// A snapshot the editor wants turned into a projection.
#[derive(Clone, Debug)]
pub struct Request {
    pub session: u64,
    pub revision: u64,
    /// The source at `revision`; shared so queuing a newer snapshot is cheap.
    pub source: Arc<str>,
    pub selection: Range<usize>,
    pub mode: ProjectionMode,
    /// The range an active native composition owns, kept literal.
    pub literal: Option<Range<usize>>,
}

impl Request {
    pub fn new(
        session: &EditSession,
        source: Arc<str>,
        selection: Range<usize>,
        mode: ProjectionMode,
        literal: Option<Range<usize>>,
    ) -> Self {
        Self {
            session: session.session_id(),
            revision: session.revision(),
            source,
            selection,
            mode,
            literal,
        }
    }
}

/// The finished work for one request.
#[derive(Clone, Debug)]
pub struct Result_ {
    pub session: u64,
    pub revision: u64,
    pub projection: Projection,
    pub semantic: Semantic,
    /// Parse and projection time, reported separately from input latency.
    pub elapsed: Duration,
}

impl Result_ {
    /// Whether this result still describes the document the editor is editing.
    pub fn still_current(&self, session: u64, revision: u64) -> bool {
        self.session == session && self.revision == revision
    }
}

/// Do the work for one request. Runs on a worker thread.
pub fn project(request: &Request) -> anyhow::Result<Result_> {
    let started = Instant::now();
    let semantic = Semantic::parse(&request.source)?;
    let projection = Projection::build_semantic(
        &request.source,
        request.selection.clone(),
        request.mode,
        request.literal.clone(),
        &semantic,
    );
    Ok(Result_ {
        session: request.session,
        revision: request.revision,
        projection,
        semantic,
        elapsed: started.elapsed(),
    })
}

/// Admission policy: one job in flight, the newest request waiting.
///
/// A newer request always replaces the waiting one, so the editor never builds
/// a snapshot the user has already typed past. The in-flight job is not
/// cancelled: parsers are not interruptible, and finishing one bounded document
/// is cheaper than restarting it.
#[derive(Default, Debug)]
pub struct Pipeline {
    active: Option<Request>,
    pending: Option<Request>,
    /// Requests superseded before they ever ran, for the drop counters.
    dropped: u64,
}

impl Pipeline {
    /// True while a job is in flight.
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    /// True when a request is waiting for the active job to finish.
    pub fn queued(&self) -> bool {
        self.pending.is_some()
    }

    /// The revision waiting to run, if any.
    pub fn pending_revision(&self) -> Option<u64> {
        self.pending.as_ref().map(|request| request.revision)
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Submit a snapshot. Returns the request to run now, if the worker is idle.
    ///
    /// A request for a revision the pipeline has already passed is ignored
    /// outright, so a late submission cannot resurrect stale work.
    pub fn submit(&mut self, request: Request) -> Option<Request> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.revision >= request.revision)
        {
            self.dropped += 1;
            return None;
        }
        match self.active.take() {
            None => {
                self.active = Some(request.clone());
                Some(request)
            }
            Some(active) => {
                if active.session == request.session && active.revision >= request.revision {
                    // The running job already covers this or a newer revision.
                    self.active = Some(active);
                    self.dropped += 1;
                    return None;
                }
                self.active = Some(active);
                if self.pending.is_some() {
                    self.dropped += 1;
                }
                self.pending = Some(request);
                None
            }
        }
    }

    /// Finish the active job and take the next one to run.
    pub fn complete(&mut self) -> Option<Request> {
        self.active = None;
        let next = self.pending.take()?;
        self.active = Some(next.clone());
        Some(next)
    }

    /// Forget everything. Used when the surface switches documents or modes.
    pub fn clear(&mut self) {
        // An in-flight job cannot be cancelled, so its slot stays occupied
        // until it reports; only the waiting snapshot is discarded.
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(revision: u64) -> Request {
        Request {
            session: 1,
            revision,
            source: Arc::from("text"),
            selection: 0..0,
            mode: ProjectionMode::Live,
            literal: None,
        }
    }

    #[test]
    fn an_idle_pipeline_runs_the_first_request_immediately() {
        let mut pipeline = Pipeline::default();
        let first = pipeline.submit(request(1));
        assert_eq!(first.map(|request| request.revision), Some(1));
        assert!(pipeline.busy());
        assert!(!pipeline.queued());
    }

    #[test]
    fn only_the_newest_waiting_snapshot_survives() {
        let mut pipeline = Pipeline::default();
        pipeline.submit(request(1)).unwrap();
        assert!(pipeline.submit(request(2)).is_none());
        assert!(pipeline.submit(request(3)).is_none());
        // Revision 2 was replaced before it ever ran.
        assert_eq!(pipeline.pending_revision(), Some(3));
        assert_eq!(pipeline.dropped(), 1);

        let next = pipeline.complete().expect("the newest snapshot runs next");
        assert_eq!(next.revision, 3);
        assert!(!pipeline.queued());
        assert!(pipeline.busy());
    }

    #[test]
    fn a_request_the_pipeline_already_passed_is_ignored() {
        let mut pipeline = Pipeline::default();
        pipeline.submit(request(5)).unwrap();
        assert!(pipeline.submit(request(4)).is_none());
        assert_eq!(pipeline.pending_revision(), None);
        assert_eq!(pipeline.dropped(), 1);
        pipeline.complete();
        assert!(!pipeline.busy());
    }

    #[test]
    fn stale_results_are_rejected_by_session_and_revision() {
        let result = Result_ {
            session: 1,
            revision: 7,
            projection: Projection::live("", 0..0),
            semantic: Semantic::default(),
            elapsed: Duration::ZERO,
        };
        assert!(result.still_current(1, 7));
        assert!(!result.still_current(1, 8), "a newer edit supersedes it");
        assert!(!result.still_current(2, 7), "another document's result");
    }

    #[test]
    fn clearing_drops_the_waiting_snapshot_but_keeps_the_active_slot() {
        let mut pipeline = Pipeline::default();
        pipeline.submit(request(1)).unwrap();
        pipeline.submit(request(2));
        pipeline.clear();
        assert_eq!(pipeline.pending_revision(), None);
        assert!(pipeline.busy(), "the running job still owns its slot");
        assert!(pipeline.complete().is_none());
        assert!(!pipeline.busy());
    }

    #[test]
    fn project_returns_the_refined_projection() {
        let request = Request {
            session: 9,
            revision: 2,
            source: Arc::from("# Title\n\n**bold** &amp;\n"),
            // A caret past the marker leaves it hidden, as the editor shows it.
            selection: 20..20,
            mode: ProjectionMode::Live,
            literal: None,
        };
        let result = project(&request).unwrap();
        assert!(result.still_current(9, 2));
        assert!(result.projection.text.starts_with("Title"));
        assert!(result.projection.text.contains("bold &"));
        assert_eq!(result.semantic.lines.len(), 3);
    }
}
