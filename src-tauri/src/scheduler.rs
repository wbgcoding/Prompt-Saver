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
    // Handles displaced by a same-name re-registration (see register_periodic). shutdown() waits
    // on these too, so a still-finishing predecessor (e.g. mid disk-write) is not cut off by exit.
    retired: Mutex<Vec<JobHandle>>,
}

impl Scheduler {
    pub fn start() -> Scheduler {
        Scheduler { jobs: Mutex::new(HashMap::new()), retired: Mutex::new(Vec::new()) }
    }

    // Registering a name twice stops the earlier job's thread and replaces it. The old handle is
    // stopped and evicted from `jobs` before the new thread is spawned, and the new thread waits
    // for the old thread's `stopped` flag before its first job() run, so a slow predecessor and the
    // new job never run concurrently.
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

    // Wakes every job thread out of its wait and waits a bounded time for all of them. False
    // means a job was still running at the deadline; the caller carries on rather than holding the
    // app open. The `stopped` flags are cloned out and the locks dropped before waiting: holding
    // `jobs` would block a concurrent retime()/register_periodic() until the deadline.
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

// One job's whole life: wait until due (or a re-time/stop arrives), run it, wait again.
// `predecessor`, when set, is the `stopped` flag of the job this one replaced under the same
// name; it is waited out before the first run so the two never run job() together.
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
            // A panicking job must not stop the schedule or skip the `stopped` update below (shutdown()'s
            // wait depends on it): one bad run is logged and the job keeps its cadence.
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
