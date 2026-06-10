//! Tokio-backed job executor for the JavaScript engine.
//!
//! Boa's default `SimpleJobExecutor` drives async jobs with a busy poll/yield
//! loop, burning a core whenever a job is waiting on I/O. This executor parks
//! instead: it runs the job queues on the global SpadeBox runtime (see
//! [`crate::runtime`]) and sleeps until either an async job completes or the
//! next timeout job is due. `Send` work spawned by jobs (e.g. `fetch` requests)
//! makes progress on the runtime's worker threads while the engine thread is
//! parked or evaluating other code.
//!
//! `run_jobs` blocks on the global runtime, so the owning `JsContext` must only
//! be driven from threads that are not themselves async executors — in
//! practice the dedicated REPL thread and the runtime's blocking pool
//! (`js_exec`).

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::mem;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use std::time::Duration;

use boa_engine::context::time::JsInstant;
use boa_engine::job::{GenericJob, Job, JobExecutor, NativeAsyncJob, PromiseJob, TimeoutJob};
use boa_engine::{Context, JsResult, JsValue};
use futures_concurrency::future::FutureGroup;
use futures_lite::Stream;

/// Outcome of waiting on the in-flight async jobs.
enum Wait {
    /// An async job ran to completion.
    Completed(Option<JsResult<JsValue>>),
    /// A pending async job enqueued new work mid-poll — synchronous jobs to
    /// drain, or a timeout earlier than the armed sleep. The main loop must
    /// run another turn before parking again.
    NewWorkEnqueued,
}

/// A FIFO job executor that parks on the SpadeBox runtime while waiting.
///
/// Bails on the first job error, like Boa's `SimpleJobExecutor`.
#[derive(Default)]
pub(super) struct SpadeboxJobExecutor {
    promise_jobs: RefCell<VecDeque<PromiseJob>>,
    async_jobs: RefCell<VecDeque<NativeAsyncJob>>,
    timeout_jobs: RefCell<BTreeMap<JsInstant, TimeoutJob>>,
    generic_jobs: RefCell<VecDeque<GenericJob>>,
}

impl SpadeboxJobExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Discards all pending jobs, called when bailing on an error.
    fn clear(&self) {
        self.promise_jobs.borrow_mut().clear();
        self.async_jobs.borrow_mut().clear();
        self.timeout_jobs.borrow_mut().clear();
        self.generic_jobs.borrow_mut().clear();
    }

    /// `true` when any queue holds a job runnable right now (excludes
    /// not-yet-due timeouts and in-flight async jobs).
    fn has_sync_work(&self) -> bool {
        !self.promise_jobs.borrow().is_empty()
            || !self.generic_jobs.borrow().is_empty()
            || !self.async_jobs.borrow().is_empty()
    }

    /// Runs all timeout jobs that are due, removing cancelled ones.
    fn run_due_timeouts(&self, context: &RefCell<&mut Context>) -> JsResult<()> {
        let now = context.borrow().clock().now();
        let mut timeouts = self.timeout_jobs.borrow_mut();
        let mut to_keep = timeouts.split_off(&now);
        to_keep.retain(|_, job| !job.is_cancelled());
        let to_run = mem::replace(&mut *timeouts, to_keep);
        drop(timeouts);

        for job in to_run.into_values() {
            if let Err(err) = job.call(&mut context.borrow_mut()) {
                self.clear();
                return Err(err);
            }
        }
        Ok(())
    }

    /// Deadline of the earliest pending timeout job, if any.
    fn earliest_deadline(&self) -> Option<JsInstant> {
        self.timeout_jobs.borrow().keys().next().copied()
    }
}

impl JobExecutor for SpadeboxJobExecutor {
    fn enqueue_job(self: Rc<Self>, job: Job, context: &mut Context) {
        match job {
            Job::PromiseJob(p) => self.promise_jobs.borrow_mut().push_back(p),
            Job::AsyncJob(a) => self.async_jobs.borrow_mut().push_back(a),
            Job::TimeoutJob(t) => {
                let now = context.clock().now();
                self.timeout_jobs.borrow_mut().insert(now + t.timeout(), t);
            }
            Job::GenericJob(g) => self.generic_jobs.borrow_mut().push_back(g),
            // `Job` is non-exhaustive; nothing in SpadeBox produces other variants.
            _ => debug_assert!(false, "unsupported job variant"),
        }
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        // Safe: callers of `run_jobs` (REPL thread, blocking pool) are never
        // async executor threads — see module docs.
        crate::runtime::handle().block_on(self.run_jobs_async(&RefCell::new(context)))
    }

    async fn run_jobs_async(self: Rc<Self>, context: &RefCell<&mut Context>) -> JsResult<()> {
        let mut group = FutureGroup::new();
        loop {
            // Move new async jobs into the group so they progress concurrently.
            for job in mem::take(&mut *self.async_jobs.borrow_mut()) {
                group.insert(job.call(context));
            }

            self.run_due_timeouts(context)?;

            // Drain the synchronous queues. Only one macrotask-equivalent pass:
            // jobs enqueued while draining are handled on the next loop turn.
            let jobs = mem::take(&mut *self.promise_jobs.borrow_mut());
            for job in jobs {
                if let Err(err) = job.call(&mut context.borrow_mut()) {
                    self.clear();
                    return Err(err);
                }
            }
            let jobs = mem::take(&mut *self.generic_jobs.borrow_mut());
            for job in jobs {
                if let Err(err) = job.call(&mut context.borrow_mut()) {
                    self.clear();
                    return Err(err);
                }
            }
            context.borrow_mut().clear_kept_objects();

            // Draining may have produced more immediately-runnable work.
            if self.has_sync_work() {
                continue;
            }

            // Arm the sleep with the earliest timeout deadline. An already-due
            // deadline yields a zero duration: the sleep fires immediately and
            // the next loop turn runs the job.
            let armed_deadline = self.earliest_deadline();
            let next_timeout = armed_deadline.map(|deadline| {
                let now = context.borrow().clock().now();
                Duration::from_millis(
                    deadline
                        .millis_since_epoch()
                        .saturating_sub(now.millis_since_epoch()),
                )
            });
            if group.is_empty() && next_timeout.is_none() {
                return Ok(()); // All queues are empty, we are done.
            }

            // Park until an async job completes or the next timeout is due.
            // Polling the group runs the job futures on this thread; if one of
            // them, without completing, enqueues synchronous work (which could
            // deadlock jobs that depend on the microtask queue) or a timeout
            // earlier than the armed sleep, stop waiting so the main loop can
            // run another turn.
            let group_is_empty = group.is_empty();
            let wait = std::future::poll_fn(|cx| match Pin::new(&mut group).poll_next(cx) {
                Poll::Ready(completion) => Poll::Ready(Wait::Completed(completion)),
                Poll::Pending
                    if self.has_sync_work() || self.earliest_deadline() != armed_deadline =>
                {
                    Poll::Ready(Wait::NewWorkEnqueued)
                }
                Poll::Pending => Poll::Pending,
            });
            tokio::select! {
                wait = wait, if !group_is_empty => {
                    if let Wait::Completed(Some(Err(err))) = wait {
                        self.clear();
                        return Err(err);
                    }
                }
                () = tokio::time::sleep(next_timeout.unwrap_or_default()), if next_timeout.is_some() => {}
                // Unreachable: we returned above when both the group and the
                // timeout queue are empty, so at least one branch is enabled.
                else => unreachable!("job executor parked with nothing to wait on"),
            }
        }
    }
}
