# Kernel Skeleton Checklist (mini plan)

Goal: the kernel will grow fast and be used; therefore **skeleton decisions that are expensive to change later** are fixed up front. Details are in the relevant files (`kernel-core.md`, `memory.md`, `drivers-io-fs-net.md`); this file is the "what must exist, in what order, under which contract" list.

Priority: **P0** = first working kernel (M2–M5), **P1** = usable system (M6–M7), **P2** = maturity (M8), **P3** = advanced.

---

## 1. User ↔ Kernel Transition (kernel entry / exit)

| # | Feature | Priority | Contract / note |
|---|---|---|---|
| 1.1 | **Trap frame** (`TrapFrame`) | P0 | All entries (syscall, IRQ, exception) save into the same structure. Defined in arch, with a user-visible subset (`UserContext`) in `include/abi` |
| 1.2 | Syscall entry/exit path | P0 | aarch64 `svc`→`sync_exception`; x86_64 `syscall/sysret`. On entry: save user regs, switch to the kernel stack, enable IRQs, `syscall_dispatch`. On exit: a single common `return_to_user()` |
| 1.3 | `return_to_user()` common exit point | P0 | Before leaving, in order: pending reschedule → pending APC/signal delivery → ptrace/trace hook → `eret/sysret`. **A single exit gate** (adding features later is easy) |
| 1.4 | Exception entry (page fault, illegal insn, alignment, FP) | P0 | If in user mode, exception → `Exception` notification to the thread (port); if in the kernel, fixup table / panic |
| 1.5 | IRQ entry | P0 | Short ISR → DPC; reschedule check on exit (`return_to_user` or kernel preempt) |
| 1.6 | Safe user access | P0 | `UserPtr<T>`, `copy_in/out`, `strncpy_from_user`; aarch64 PAN, x86 SMAP; fixup table |
| 1.7 | vDSO / shared page | P1 | `clock_gettime`, `getpid`-like queries without a syscall; mapped read-only into the user |
| 1.8 | Kernel stack overflow protection | P0 | Guard page, separate IRQ stack |
| 1.9 | Syscall filter/seccomp-like hook | P2 | Set of syscalls allowed per Job/Token (sandbox) |

## 2. Context Switch & Execution Contexts

| # | Feature | Priority | Note |
|---|---|---|---|
| 2.1 | `arch_switch(prev, next)` | P0 | callee-saved + SP + address space (TTBR0/CR3, ASID/PCID to reduce TLB flushes) |
| 2.2 | First-entry stub for a new thread | P0 | `thread_entry_trampoline` — kernel threads and user threads take the same path |
| 2.3 | FPU/SIMD/SVE management | P0/P1 | Save user state on switch; no SIMD in the kernel; `kernel_fpu_begin/end` |
| 2.4 | Thread-local storage support | P0 | `TPIDR_EL0` / `FS base`; kernel per-CPU via `TPIDR_EL1` / `GS base` |
| 2.5 | Preemption model | P0 | `preempt_count`; the kernel is preemptible; the `need_resched` flag is checked on `return_to_user`/IRQ exit |
| 2.6 | Yield / sleep / wake / block | P0 | `sched_yield`, `sleep_until`, `wait(obj)` all land in the same `block()/wake()` core |
| 2.7 | Kernel threads & worker pool | P0 | idle, reaper, writeback, `workqueue` (work that may block, from a DPC) |
| 2.8 | Concurrent execution units | P1 | Basis for fibers / user-mode scheduling (UMS): a `thread_switch_to(tid)` fast path for the user (P3) |
| 2.9 | CPU hotplug / SMP IPI kinds | P1 | reschedule, TLB shootdown, call-function, panic-stop |

## 3. Syscall Surface (families)

Handle-based, small; numbers live in `include/abi/syscall.h` and are **never reused**. Every call takes `(args…, flags)`; growing structures can gain fields through `{u32 size; u32 version; …}`.

| Family | Main calls | Pri |
|---|---|---|
| Object | `ObjClose, ObjDup, ObjQuery, ObjWait, ObjWaitMany, ObjSignal, ObjName*` | P0 |
| Process | `ProcSpawn, ProcExit, ProcQuery, ProcKill, ProcSuspend, ProcWait` | P0 |
| Thread | `ThreadCreate, ThreadExit, ThreadYield, ThreadSleep, ThreadQuery, ThreadSetAffinity/Prio, ThreadSuspend` | P0 |
| Memory | `MemMap, MemUnmap, MemProtect, MemQuery, MemAdvise, SectionCreate/Open, MemLock, MemRemap` | P0 |
| Sync | `FutexWait/Wake/Requeue, EventCreate/Set/Reset, SemCreate/Release, MutexCreate, TimerCreate/Set` | P0 |
| I/O (ring) | `IoRingCreate, IoRingEnter, IoRingRegister (buffer/file)`, `IoCancel` | P0 (ring), P1 (full) |
| File | `FileOpen, FileStat, FileSeek, FileTruncate, DirRead, Rename, Unlink, Link, Mount, Sync` | P1 (mostly over the ring) |
| Network | `SockCreate, Bind, Listen, Connect, Accept, SendMsg, RecvMsg, SockOpt` | P1 (over the ring) |
| IPC | `PortCreate, PortSend, PortReceive, PortReply, SharedMemory` | P0 |
| Time | `ClockGet, ClockSetAlarm, TimerCreate` | P0 |
| Security | `TokenQuery, TokenAdjust, Impersonate (advanced)` | P1 |
| Resource | `JobCreate, JobAssign, JobSetLimits, JobQuery` | P2 |
| System | `SysInfo, SysLogRead, SysReboot, ModuleLoad/Unload, SysctlGet/Set` | P1 |
| Debug | `DebugAttach, ThreadGetContext/SetContext, MemRead/Write(debug), TraceCtl` | P1 |
| Randomness | `GetRandom` | P0 |
| Graphics | `GpuEnum, GpuBo*, GpuSubmit, GpuFence*, Disp*` | G1 (separate, `graphics.md`) |

Rules: (a) error returns are negative `errno-like` values from a single set (`include/abi/status.h`), (b) all blocking calls take a timeout + cancellation, (c) every syscall automatically gets a `trace` point and a `count` counter, (d) POSIX `fork/exec/signal` are not in the kernel; the libc layer sits on top.

## 4. Asynchronous & Event System (epoll / io_uring equivalent)

All of these are built on **a single foundation**: **`IoRing` + `WaitSet`**. Separate epoll/aio/poll/select are not written.

| # | Feature | Pri | Note |
|---|---|---|---|
| 4.1 | **IoRing** (io_uring-like) | P0→P1 | Shared-memory SQ/CQ, `IoRingEnter(submit, min_complete, timeout)`. Opcodes: `NOP, READ, WRITE, FSYNC, ACCEPT, CONNECT, SEND, RECV, POLL_ADD, TIMEOUT, CANCEL, FUTEX_WAIT, OPENAT, CLOSE, MMAP-less splice`, linked SQEs, registered buffers/files, **multishot**, a mode with no syscalls at all for the user (SQ polling) |
| 4.2 | **WaitSet** (epoll equivalent) | P0 | `WaitSetCreate/Add(obj, mask, key)/Remove/Wait`. Edge/level/oneshot. Every dispatcher object (`Event, Timer, Socket, File, Port, Process, Thread`) can be added to a `WaitSet`. The IoRing `POLL_ADD` uses the same mechanism |
| 4.3 | Waitable objects | P0 | `Event, Semaphore, Timer, Process/Thread exit, Port, Socket, Pipe, File(change), Signal` — all share the `Waitable` interface: `poll(mask)`, `add_waiter`, `remove_waiter` |
| 4.4 | `ObjWaitMany` | P0 | The simple form of WaitSet (one-shot, small set) |
| 4.5 | Futex | P0 | `wait/wake/requeue/wake_op`, PI futex, timeout, process-shared (via section) |
| 4.6 | Timer object | P0 | one-shot/periodic, high resolution, Waitable |
| 4.7 | Eventfd / Pipe / Socketpair equivalents | P1 | `Event` (counter mode), `Pipe`, `Port` already exists |
| 4.8 | Cancellation infrastructure | P0 | Every in-flight `IoRequest` is cancellable (`IoCancel`); thread terminate → pending requests are cancelled |
| 4.9 | Notification: APC / user event | P0 | Asynchronous user-mode callback to a thread, delivered on `return_to_user` exit |
| 4.10 | File change notification (inotify) | P2 | VFS hook → Waitable |
| 4.11 | Completion port semantics | P1 | An IoRing CQ can be shared by several threads (IOCP-style pool); matching via `user_data` |

## 5. Process Lifecycle & Resource Model

| # | Feature | Pri |
|---|---|---|
| 5.1 | Spawn + handle inheritance + argv/env + auxv | P0 |
| 5.2 | Exit, wait, zombie management, reaper | P0 |
| 5.3 | Process hierarchy (parent/job) + "init" adopts orphaned children | P0 |
| 5.4 | Dynamic linker (userspace `ld`) + interpreter loading | P1 |
| 5.5 | Job (cgroup equivalent): memory/CPU/thread/IO limits, bulk kill | P2 |
| 5.6 | Namespaces (pid, mount, net, ipc, user) | P3 (the object path abstraction is designed to be ready for this) |
| 5.7 | Core dump / crash report | P2 |
| 5.8 | Debug/trace (ptrace equivalent: `DebugAttach`) | P1 |
| 5.9 | Exception port (SEH / signal alternative) + optional POSIX signal layer in libc | P1 |

## 6. Memory Skeleton (summary, details in `memory.md`)

P0: PMM, slab/`kmalloc`, `vmalloc`, own page tables, VMA, anonymous memory, page fault, COW, guard page.
P1: Section/mmap, page cache, `MemAdvise`, huge pages (2M/1G; aarch64 contiguous), zero-page sharing, `MemLock`.
P2: Swap/zram, memory cgroup (Job), NUMA, KASLR, KASAN-like debugging, KSM (P3).
Fixed contract: all allocation calls take `GfpFlags` (atomic/blocking/dma32/zero).

## 7. Time, Randomness, Power

- P0: monotonic + realtime clock, tickless timer, `GetRandom` (hardware RNG: aarch64 `RNDR`, x86 `RDRAND` + UEFI + timing entropy, CSPRNG).
- P1: RTC (UEFI runtime / PL031 / CMOS), NTP in userspace; timer slack.
- P2: CPU frequency/idle states (cpuidle), suspend/resume, ACPI/PSCI shutdown-reboot.

## 8. Security Skeleton

P0: user/kernel separation, W^X, PAN/PXN, SMAP/SMEP, handle rights, stack guard.
P1: Token (user/group/capability), file ACL, `Se` access-check hooks (`se_check(token, obj, rights)`).
P2: stack canary, CFI/shadow stack (aarch64 PAC/BTI, x86 CET), KASLR, kernel lockdown mode, signed modules.
P3: capability-based sandbox, MAC policy, TPM/measured boot.

## 9. Driver / Module / Platform Skeleton

P0: versioned ODK ABI, device tree, IRQ/DMA/MMIO API, static drivers.
P1: loadable modules (symbol export, version check), PCI/virtio, DT + ACPI enumeration, hotplug events.
P2: IOMMU (SMMU/VT-d), MSI-X, power management hooks, user-mode driver framework.
Driver event notification (uevent-like) → `svcmgr`.

## 10. Observability & Debugging

P0: serial log, panic + backtrace, assert macros, `kdebug` command line (serial).
P1: kernel log ring + `SysLogRead`, per-syscall counters, ktest framework, QEMU gdbstub scripts, symbolizer.
P2: trace points (ftrace/ETW-like ring), perf counters (PMU), lockdep, memory leak detector, an `/sys`-like introspection filesystem (sysfs equivalent: `objfs` — show the object tree as files).
P3: kdump/crash dump, live patch.

## 11. Rules for Growth Readiness (MANDATORY for fast growth)

1. **Frozen ABI layers:** `include/abi`, `include/uapi`, `include/odk` are versioned; a change = a new field/new syscall, never break the old one. Every PR that changes `abi` needs an `ABI_CHANGELOG.md` entry.
2. **Extensible structures:** every user-visible struct starts with `{size, version, flags}`; an unknown flag → error (no silent ignoring).
3. **Feature flags:** `SysInfo(FEATURES)` lets the user ask which features exist (e.g. `IORING_MULTISHOT`).
4. **Every new object type** follows the `Object` + `Waitable` (if needed) + handle rights + `ObjQuery` contract; this pattern is tried before opening a special syscall.
5. **A single entry gate and a single exit gate:** syscall/IRQ/exception → `enter_kernel()`; exit → `return_to_user()`. New features (trace, seccomp, signals) are added as hooks at these two points.
6. **Lock order and context rules** (`kernel-core.md` §4) are enforced by lockdep; a new subsystem adds its lock to the order.
7. **Every subsystem:** an `init()` stage (boot order table: `early → mm → sched → driver → fs → net → user`), a test suite, an introspection endpoint, a log tag.
8. **Prevent boilerplate problems:** the syscall table is generated from a single source (`syscalls.def`) → kernel dispatcher, `libsys` stubs, strace/trace naming, Rust bindings, all automatic.
9. **Performance budget:** benchmarks (`tests/bench`) for empty syscall, context switch, IPC ping-pong and ring submit/complete track regressions in CI.
10. **No undocumented behavior:** a syscall is not merged without `docs/syscalls/<name>.md` (auto-generated skeleton).

## 12. Acceptance Map (which rows in which milestone)

| Milestone | P0 items covered |
|---|---|
| M2 | 1.1, 1.4, 1.5, 1.8, 2.5 (skeleton), 10 (P0) |
| M3 | 6 (P0), 1.6 |
| M4 | 2.1–2.7, 4.5 (internal), 4.6, kernel sync, 4.3 `Waitable` core |
| M5 | 1.2, 1.3, 3 (Object/Process/Thread/Memory/Sync/Time/Random/IPC), 4.2, 4.4, 4.5, 4.9, 5.1–5.3 |
| M6 | 4.1 (ring core + file opcodes), 9 (P0/P1), 3 (File) |
| M7 | 4.1 network opcodes, 3 (Network), 4.11 |
| M8 | P2 items: 1.9, 5.5, 5.7, 8, 10 |
