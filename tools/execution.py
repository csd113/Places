"""Small, bounded spawn executor for offline tools (Python 3.10+).

Run heavy tools sequentially. PLACES_TOOL_WORKERS is the aggregate allocation
given to a tool by its caller, never an additional pool at each nesting level.
Workers only compute; the caller validates results before publishing outputs.
"""
import multiprocessing
from concurrent.futures import ProcessPoolExecutor, wait, FIRST_EXCEPTION
from pathlib import Path
import tempfile
import stat
import signal
import os
import sys


def worker_count(requested=None, jobs=None, automatic=12):
    available = getattr(os, "process_cpu_count", os.cpu_count)() or 1
    allocation = os.environ.get("PLACES_TOOL_WORKERS", "12")
    try:
        budget = int(allocation)
        count = int(requested) if requested is not None else automatic
    except (ValueError, TypeError) as error:
        raise ValueError("workers and PLACES_TOOL_WORKERS must be positive integers") from error
    if budget < 1 or count < 1:
        raise ValueError("workers and PLACES_TOOL_WORKERS must be positive integers")
    return max(1, min(count, budget, available, 12, jobs if jobs is not None else 12))


def _initialize(initializer, arguments):
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    # Children own one slot; prevent nested tools/native libraries multiplying it.
    for name in ("PLACES_TOOL_WORKERS", "OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS",
                 "MKL_NUM_THREADS", "VECLIB_MAXIMUM_THREADS", "NUMEXPR_NUM_THREADS"):
        os.environ[name] = "1"
    if initializer is not None:
        initializer(*arguments)


def ordered_map(function, jobs, workers=None, *, initializer=None, initargs=(),
                automatic=12, progress=None):
    """Return stable results; bounded submissions and pool cleanup on any failure.

    A batch holds at most two jobs per worker. A worker is reused across batches;
    this bounds queued payloads without requiring a new scheduler or Python 3.14.
    """
    jobs = list(jobs)
    count = worker_count(workers, len(jobs), automatic)
    if not jobs:
        return []
    results = []
    if count == 1:
        if initializer is not None:
            initializer(*initargs)
        for job in jobs:
            results.append(function(job))
    else:
        pool = ProcessPoolExecutor(max_workers=count, mp_context=multiprocessing.get_context("spawn"),
                                   initializer=_initialize, initargs=(initializer, initargs))
        try:
            for start in range(0, len(jobs), count * 2):
                pending = [pool.submit(function, job) for job in jobs[start:start + count * 2]]
                wait(pending, return_when=FIRST_EXCEPTION)
                for future in pending:
                    if future.done() and future.exception() is not None:
                        raise future.exception()
                results.extend(future.result() for future in pending)
                if progress:
                    print(f"[{progress}] completed {len(results)}/{len(jobs)}", file=sys.stderr)
        except BaseException:
            # Python 3.10–3.13 have no terminate_workers API. These are
            # exclusively this executor's owned spawn processes. Capture
            # them before shutdown clears the mapping, then join all.
            owned = list((pool._processes or {}).values())
            for process in owned:
                if process.is_alive():
                    process.terminate()
            for process in owned:
                process.join()
            raise
        finally:
            pool.shutdown(wait=True, cancel_futures=True)
    return results


def atomic_write(path, payload):
    """Publish one complete encoded output, removing only our own temp on failure."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix="." + path.name + ".", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            os.fchmod(handle.fileno(), stat.S_IMODE(path.stat().st_mode) if path.exists() else 0o644)
            handle.write(payload)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
