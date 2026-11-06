//! Threads and a single-CPU preemptive round-robin scheduler.
//!
//! Lifecycle: spawn -> Ready <-> Running; sleep_ms -> Sleeping; join -> Blocked;
//! suspend/resume toggle a flag the scheduler honours; exit/terminate -> Dead (reaped
//! lazily, since a thread cannot free the stack it is standing on).
//!
//! All state is only touched with interrupts disabled (single CPU). SMP will replace
//! this with per-CPU run queues plus spinlocks.

use crate::arch::{context, cpu};
use crate::kprintln;
use crate::mm::heap::{kfree, kmalloc};
use alloc::{boxed::Box, vec::Vec};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

pub type Tid = u32;

pub const STACK_SIZE: usize = 32 * 1024;
const SLICE_TICKS: u32 = 10; // 10 ms at 1 kHz

pub static TICKS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ready,
    Running,
    Sleeping,
    Blocked,
    Dead,
}

pub struct Thread {
    pub tid: Tid,
    pub name: &'static str,
    pub state: State,
    pub suspended: bool,
    rsp: usize,
    stack: *mut u8,
    wake_tick: u64,
    wait_for: Option<Tid>,
    entry: Option<fn(usize)>,
    arg: usize,
    cpu_ticks: u64,
}

impl Drop for Thread {
    fn drop(&mut self) {
        if !self.stack.is_null() {
            // SAFETY: allocated by kmalloc in `spawn`; the thread is not running.
            unsafe { kfree(self.stack) };
        }
    }
}

struct Sched {
    threads: Vec<Box<Thread>>,
    current: Tid,
    idle: Tid,
    next_tid: Tid,
    slice: u32,
}

struct Global(UnsafeCell<Option<Sched>>);
// SAFETY: accessed only with interrupts disabled on a single CPU.
unsafe impl Sync for Global {}
static SCHED: Global = Global(UnsafeCell::new(None));

fn sched() -> &'static mut Sched {
    // SAFETY: see `Global`. Callers keep interrupts disabled and never hold two borrows across a call.
    unsafe { (*SCHED.0.get()).as_mut().expect("scheduler not initialised") }
}

fn ready() -> bool {
    // SAFETY: as above, read-only check.
    unsafe { (*SCHED.0.get()).is_some() }
}

impl Sched {
    fn idx(&self, tid: Tid) -> Option<usize> {
        self.threads.iter().position(|t| t.tid == tid)
    }
    fn cur(&mut self) -> &mut Thread {
        let i = self.idx(self.current).unwrap();
        &mut self.threads[i]
    }
    fn wake_joiners(&mut self, dead: Tid) {
        for t in self.threads.iter_mut() {
            if t.state == State::Blocked && t.wait_for == Some(dead) {
                t.state = State::Ready;
                t.wait_for = None;
            }
        }
    }
}

fn new_thread(s: &mut Sched, name: &'static str, entry: Option<fn(usize)>, arg: usize, stack: bool) -> Tid {
    let tid = s.next_tid;
    s.next_tid += 1;
    let (mem, rsp) = if stack {
        // SAFETY: fresh allocation owned by the new thread.
        let m = unsafe { kmalloc(STACK_SIZE) };
        assert!(!m.is_null(), "out of memory for thread stack");
        (m, context::init_stack(m as usize + STACK_SIZE, thread_start))
    } else {
        (core::ptr::null_mut(), 0)
    };
    s.threads.push(Box::new(Thread {
        tid,
        name,
        state: State::Ready,
        suspended: false,
        rsp,
        stack: mem,
        wake_tick: 0,
        wait_for: None,
        entry,
        arg,
        cpu_ticks: 0,
    }));
    tid
}

extern "sysv64" fn thread_start() -> ! {
    cpu::enable_interrupts();
    let (f, a) = cpu::without_interrupts(|| {
        let t = sched().cur();
        (t.entry.unwrap(), t.arg)
    });
    f(a);
    exit()
}

fn idle_main(_: usize) {
    loop {
        cpu::enable_and_hlt();
    }
}

/// Adopt the boot context as thread 1 and create the idle thread (tid 0).
pub fn init() {
    cpu::without_interrupts(|| {
        let mut s = Sched { threads: Vec::new(), current: 1, idle: 0, next_tid: 0, slice: SLICE_TICKS };
        new_thread(&mut s, "idle", Some(idle_main), 0, true);
        let boot = new_thread(&mut s, "boot", None, 0, false);
        s.cur_set_running(boot);
        // SAFETY: interrupts are off, nothing else can observe the cell.
        unsafe { *SCHED.0.get() = Some(s) };
    });
}

impl Sched {
    fn cur_set_running(&mut self, tid: Tid) {
        self.current = tid;
        let i = self.idx(tid).unwrap();
        self.threads[i].state = State::Running;
    }
}

pub fn spawn(name: &'static str, entry: fn(usize), arg: usize) -> Tid {
    cpu::without_interrupts(|| new_thread(sched(), name, Some(entry), arg, true))
}

pub fn current() -> Tid {
    cpu::without_interrupts(|| sched().current)
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Pick the next thread and switch. Interrupts must be disabled. The caller has
/// already set the current thread's state (Ready, Sleeping, Blocked or Dead).
fn schedule() {
    let (old, new) = {
        let s = sched();
        // Reap dead threads (never the current one: we are on its stack).
        let cur = s.current;
        s.threads.retain(|t| t.state != State::Dead || t.tid == cur);

        let n = s.threads.len();
        let start = s.idx(cur).unwrap() + 1;
        let mut pick = s.idx(s.idle).unwrap();
        for k in 0..n {
            let i = (start + k) % n;
            let t = &s.threads[i];
            if t.tid != s.idle && t.state == State::Ready && !t.suspended {
                pick = i;
                break;
            }
        }
        let ci = s.idx(cur).unwrap();
        s.slice = SLICE_TICKS;
        if pick == ci {
            s.threads[ci].state = State::Running;
            return;
        }
        s.threads[pick].state = State::Running;
        s.current = s.threads[pick].tid;
        (&mut s.threads[ci].rsp as *mut usize, s.threads[pick].rsp)
    };
    // SAFETY: `old` points into a live Box<Thread>; `new` is a saved/prepared stack pointer.
    unsafe { context::switch_context(old, new) };
}

/// Called from the timer interrupt (interrupts disabled).
pub fn tick() {
    let now = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    if !ready() {
        return;
    }
    let s = sched();
    for t in s.threads.iter_mut() {
        if t.state == State::Sleeping && t.wake_tick <= now {
            t.state = State::Ready;
        }
    }
    let idle = s.idle;
    let cur = s.cur();
    cur.cpu_ticks += 1;
    let cur_tid = cur.tid;
    if s.slice > 0 {
        s.slice -= 1;
    }
    if s.slice == 0 || cur_tid == idle {
        let c = s.cur();
        if c.state == State::Running {
            c.state = State::Ready;
        }
        schedule();
    }
}

pub fn yield_now() {
    cpu::without_interrupts(|| {
        let c = sched().cur();
        if c.state == State::Running {
            c.state = State::Ready;
        }
        schedule();
    });
}

pub fn sleep_ms(ms: u64) {
    if ms == 0 {
        return yield_now();
    }
    cpu::without_interrupts(|| {
        let now = ticks();
        let c = sched().cur();
        c.wake_tick = now + ms; // 1 tick = 1 ms
        c.state = State::Sleeping;
        schedule();
    });
}

/// Block until `tid` has exited (returns immediately if it already has / never existed).
pub fn join(tid: Tid) {
    loop {
        let done = cpu::without_interrupts(|| {
            let s = sched();
            match s.idx(tid) {
                Some(i) if s.threads[i].state != State::Dead && tid != s.current => {
                    let c = s.cur();
                    c.state = State::Blocked;
                    c.wait_for = Some(tid);
                    schedule();
                    false
                }
                _ => true,
            }
        });
        if done {
            return;
        }
    }
}

pub fn suspend(tid: Tid) {
    cpu::without_interrupts(|| {
        let s = sched();
        if let Some(i) = s.idx(tid) {
            if tid == s.idle {
                return;
            }
            s.threads[i].suspended = true;
            if tid == s.current {
                s.threads[i].state = State::Ready;
                schedule();
            }
        }
    });
}

pub fn resume(tid: Tid) {
    cpu::without_interrupts(|| {
        let s = sched();
        if let Some(i) = s.idx(tid) {
            s.threads[i].suspended = false;
        }
    });
}

/// Stop another thread (or exit, if it is the caller).
pub fn terminate(tid: Tid) {
    if tid == current() {
        exit();
    }
    cpu::without_interrupts(|| {
        let s = sched();
        if tid == s.idle {
            return;
        }
        if let Some(i) = s.idx(tid) {
            s.threads[i].state = State::Dead;
            s.wake_joiners(tid);
        }
    });
}

pub fn exit() -> ! {
    cpu::disable_interrupts();
    let s = sched();
    let tid = s.current;
    s.cur().state = State::Dead;
    s.wake_joiners(tid);
    schedule();
    unreachable!("dead thread was rescheduled");
}

pub fn dump() {
    cpu::without_interrupts(|| {
        kprintln!("  tid name       state      susp  cpu_ms");
        for t in sched().threads.iter() {
            kprintln!("  {:<3} {:<10} {:<10} {:<5} {}", t.tid, t.name, alloc::format!("{:?}", t.state), t.suspended, t.cpu_ticks);
        }
    });
}
