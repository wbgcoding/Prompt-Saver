// Small data-parallel helpers on std scoped threads, sized by `available_parallelism()`.
// They replace a general work-stealing pool: the three callers (screen-capture pixel swap,
// duplicate scan, batch probing) each have one big, evenly or dynamically divisible loop.

use std::sync::atomic::{AtomicUsize, Ordering};

fn available_workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

// Runs `f(0)`, `f(1)`, ... `f(n - 1)` on up to one thread per core and returns the results in
// index order. Indices are handed out one at a time, so uneven work (a triangular pair scan, a
// slow file probe) still balances across the threads. A panic in `f` propagates to the caller.
pub(crate) fn par_map_indexed<R: Send>(n: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    let workers = available_workers().min(n);
    if workers <= 1 {
        return (0..n).map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let parts: Vec<Vec<(usize, R)>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= n {
                            break;
                        }
                        mine.push((i, f(i)));
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    });
    let mut slots: Vec<Option<R>> = (0..n).map(|_| None).collect();
    for (i, r) in parts.into_iter().flatten() {
        slots[i] = Some(r);
    }
    slots.into_iter().flatten().collect()
}

// Same as `par_map_indexed` over the elements of a slice.
pub(crate) fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    par_map_indexed(items.len(), |i| f(&items[i]))
}

// A buffer below this size is processed on the calling thread: a thread start costs more than
// the work. Above it the buffer is cut into slabs of this size that the threads take in turn,
// so a thread that gets descheduled does not hold up the rest.
const SLAB: usize = 1 << 18;

// Cuts `data` into slabs of about 256 KiB, each a whole multiple of `unit` elements, and runs
// `f` on every slab, spread over up to one thread per core. A trailing partial unit (if
// `data.len()` is not a multiple of `unit`) rides along in the last slab.
pub(crate) fn par_slabs_mut<T: Send>(data: &mut [T], unit: usize, f: impl Fn(&mut [T]) + Sync) {
    let unit = unit.max(1);
    let slab = (SLAB / unit).max(1) * unit;
    let workers = available_workers().min(data.len() / slab).max(1);
    if workers <= 1 {
        f(data);
        return;
    }
    let whole = data.len() / slab * slab;
    let (body, tail) = data.split_at_mut(whole);
    let queue = std::sync::Mutex::new(body.chunks_mut(slab));
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| loop {
                    let next = queue.lock().unwrap_or_else(|e| e.into_inner()).next();
                    match next {
                        Some(chunk) => f(chunk),
                        None => break,
                    }
                })
            })
            .collect();
        if !tail.is_empty() {
            f(tail);
        }
        for h in handles {
            if let Err(e) = h.join() {
                std::panic::resume_unwind(e);
            }
        }
    });
}
