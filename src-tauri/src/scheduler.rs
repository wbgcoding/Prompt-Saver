use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

type Job = Box<dyn Fn() + Send + 'static>;

// A zero interval would turn a job's loop into a busy-wait.
const MIN_INTERVAL: Duration = Duration::from_millis(1);

enum JobMsg {
    Retime(Duration),
    Stop,
}

struct JobHandle {
    tx: Sender<JobMsg>,
    stopped: Arc<AtomicBool>,
}

pub struct Scheduler {
    jobs: Mutex<HashMap<String, JobHandle>>,
    // Handles displaced by a same-name re-registration (see register_periodic).
    // shutdown() keeps waiting on these too, or a re-registered job's
    // still-finishing predecessor thread (e.g. mid disk-write) could be cut
    // off by process exit once its map entry is gone.
    retired: Mutex<Vec<JobHandle>>,
}

impl Scheduler {
    pub fn start() -> Scheduler {
        Scheduler { jobs: Mutex::new(HashMap::new()), retired: Mutex::new(Vec::new()) }
    }

    // Registering a name twice stops the earlier job's thread and replaces
    // it rather than running two copies of it. The old handle is stopped
    // and evicted from `jobs` BEFORE the new thread is even spawned, and the
    // new thread is additionally told to wait for that old thread's `stopped`
    // flag before running its own job() for the first time -- closing the
    // window where a slow-to-notice predecessor (mid `job()` when re-
    // registered) and the new job could otherwise run concurrently.
    pub fn register_periodic(
        &self,
        name: &str,
        first_delay: Duration,
        interval: Duration,
        job: impl Fn() + Send + 'static,
    ) {
        let interval = interval.max(MIN_INTERVAL);
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let old = jobs.remove(name);
        let predecessor = old.as_ref().map(|h| {
            let _ = h.tx.send(JobMsg::Stop);
            h.stopped.clone()
        });
        if let Some(old) = old {
            let mut retired = self.retired.lock().unwrap_or_else(|e| e.into_inner());
            retired.retain(|h| !h.stopped.load(Ordering::Acquire)); // self-cleaning
            retired.push(old);
        }
        let (tx, rx) = channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = stopped.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("job-{name}"))
            .spawn(move || job_loop(rx, first_delay, interval, Box::new(job), flag, predecessor));
        if spawned.is_err() {
            // No thread means nothing to wait for at shutdown.
            stopped.store(true, Ordering::Release);
            return;
        }
        jobs.insert(name.to_string(), JobHandle { tx, stopped });
    }

    // The new interval counts from the job's last run, so a shortened one can
    // make it due immediately instead of serving out the old wait first.
    pub fn retime(&self, name: &str, interval: Duration) {
        let interval = interval.max(MIN_INTERVAL);
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = jobs.get(name) {
            let _ = h.tx.send(JobMsg::Retime(interval));
        }
    }

    // Wakes every job thread out of its wait and waits a bounded time for
    // all of them to finish. False means at least one job was still running
    // when the deadline passed; the caller carries on rather than holding
    // the app open for it. The `stopped` flags are cloned out and the locks
    // dropped before the wait loop -- holding `jobs` for the whole wait would
    // block any concurrent retime()/register_periodic() call (e.g. from a job
    // thread's own last run) until the deadline, for no reason at shutdown.
    pub fn shutdown(&self, wait: Duration) -> bool {
        let flags: Vec<Arc<AtomicBool>> = {
            let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
            for h in jobs.values() {
                let _ = h.tx.send(JobMsg::Stop);
            }
            let retired = self.retired.lock().unwrap_or_else(|e| e.into_inner());
            jobs.values().map(|h| h.stopped.clone()).chain(retired.iter().map(|h| h.stopped.clone())).collect()
        };
        let deadline = Instant::now() + wait;
        loop {
            if flags.iter().all(|f| f.load(Ordering::Acquire)) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

// One job's entire life: wait until due (or a re-time/stop arrives early),
// run it, wait again. Nothing here ever touches another job's state.
// `predecessor`, when set, is the `stopped` flag of the job this one just
// replaced under the same name -- waited out before the first run so the
// two never execute job() at the same time (see register_periodic).
fn job_loop(
    rx: Receiver<JobMsg>,
    first_delay: Duration,
    mut interval: Duration,
    job: Job,
    stopped: Arc<AtomicBool>,
    predecessor: Option<Arc<AtomicBool>>,
) {
    if let Some(pred) = predecessor {
        while !pred.load(Ordering::Acquire) {
            // A Stop for THIS job while still waiting on its predecessor means
            // it was itself immediately superseded; nothing to run, just exit.
            match rx.recv_timeout(Duration::from_millis(5)) {
                Ok(JobMsg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    stopped.store(true, Ordering::Release);
                    return;
                }
                _ => {}
            }
        }
    }
    let mut last_run = Instant::now();
    let mut due = last_run + first_delay;
    loop {
        let wait = due.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(JobMsg::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(JobMsg::Retime(new_interval)) => {
                interval = new_interval;
                due = last_run + interval;
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if due <= Instant::now() {
            // A panicking job must not silently stop the whole schedule (and
            // must not skip the `stopped` update below, which shutdown()'s
            // wait loop depends on) -- one bad run is logged and the job
            // keeps its cadence instead of the thread quietly dying.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(&job)).is_err() {
                super::append_crash_log("scheduled job panicked; it will still run again on its normal interval");
            }
            last_run = Instant::now();
            due = last_run + interval;
        }
    }
    stopped.store(true, Ordering::Release);
}

static GLOBAL: OnceLock<Scheduler> = OnceLock::new();

fn global() -> &'static Scheduler {
    GLOBAL.get_or_init(Scheduler::start)
}

pub fn register_periodic(
    name: &str,
    first_delay: Duration,
    interval: Duration,
    job: impl Fn() + Send + 'static,
) {
    global().register_periodic(name, first_delay, interval, job);
}

pub fn retime(name: &str, interval: Duration) {
    global().retime(name, interval);
}

// Never started (no periodic job was ever registered) means nothing to stop.
pub fn shutdown(wait: Duration) -> bool {
    GLOBAL.get().map_or(true, |s| s.shutdown(wait))
}
