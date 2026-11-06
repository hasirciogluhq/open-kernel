# Kernel Core: CPU, Thread, Scheduler, Sync, Process, Syscall

Language priority is **Rust > C++ > C**. `ke` (scheduler/sync/dispatcher) is C++26, `ps/ob/io/ipc/se/syscall` are Rust, arch entry/exit is C + asm. The C++ side uses `-fno-exceptions -fno-rtti`; errors are returned as `Result<T>`.

## 1. CPU & Interrupts (arch + hal)

- **Early init:** exception vectors, FPU/SIMD permissions, per-CPU area (aarch64 `TPIDR_EL1`, x86_64 `GS base`) → `current_cpu()` and `current_thread()` in a single instruction.
- **Interrupt controller:** GICv3 (aarch64) / LAPIC+IOAPIC (x86_64) → one interface in the HAL: `irq_register(vec, handler)`, `irq_mask/unmask`, `ipi_send(cpu, kind)`.
- **Timer:** aarch64 generic timer, x86_64 LAPIC timer (TSC-deadline) → a single `hal_timer_set_oneshot(ns)`; tickless idle.
- **SMP:** aarch64 PSCI `CPU_ON`, x86_64 INIT-SIPI (ACPI MADT). Each CPU gets its own stack + per-CPU data.
- **Atomics:** `std::atomic` + memory barriers in `arch/common` (`dmb/dsb/isb`, `mfence`); aarch64 LSE is used when available.
- **Interrupt levels (IRQL-lite):** `passive` < `dispatch` < `device` < `high`. The level is raised when taking a spinlock; a blocking call at the wrong level asserts in debug builds.

## 2. Thread

```
Thread { id, state(ready/running/blocked/dead), prio, class, cpu_affinity,
         kstack, user_ctx(arch), saved_ctx(arch), wait_block, owner Process*, ob header }
```

- Context switch: a single function in arch, `arch_switch(prev, next)` — callee-saved regs + stack pointer + (if the user address space changed) `TTBR0` / `CR3`. FPU/SIMD state is **not lazy**: on aarch64 it is saved/restored on the switch to user, and the kernel does not use SIMD (only inside `kernel_fpu_begin/end` regions).
- Kernel threads (idle, reaper, worker) use the same `Thread` type.
- APC-like: asynchronous notifications to a user thread (cancellation, exceptions) are delivered on return to user mode.

## 3. Scheduler

- **Per-CPU runqueue**, with a CPU-local lock. Two classes:
  - `realtime`: 32 fixed priorities, preemptive, FIFO/RR.
  - `fair`: EEVDF-like (virtual deadline + lag), nice value as weight.
- Idle thread: `wfi` / `hlt` (+ mwait later).
- Load balancing: an idle CPU does work stealing; on wakeup, a thread prefers a CPU near the one it last ran on.
- Preemption: via timer tick + wakeup; the kernel is preemptible while `preempt_count` is zero.
- Later: CPU grouping (capacity information for big.LITTLE / Apple P/E cores).

## 4. Synchronization

| Primitive | Note |
|---|---|
| `SpinLock`, `TicketLock`, `RwSpin` | IRQ-safe variants (`lock_irqsave`) |
| `Mutex` (adaptive, priority inheritance) | may block; at passive level |
| `Event`, `Semaphore`, `Timer` objects | Windows dispatcher style; exposed to userspace via handles |
| `wait_many(objs[], any/all, timeout)` | the single wait mechanism |
| `Futex` | fast path for user mutex/condvar; address+key hash table |
| `RCU` (simple, quiescent-state based) | read-mostly places such as dentry/route/device lookups |
| `Seqlock`, `PerCpu<T>` | counters, clock |

Rule: the lock order is documented (`ke < mm < ob < ps < io < vfs/net`); debug builds have a lockdep-like order checker.

## 5. Process

```
Process { pid, AddressSpace*, HandleTable, threads[], token(se), job*, exit_code, image info }
```

- `ProcCreate(image, args, handles_to_inherit)` — **no fork**; Windows-style `spawn`. (POSIX `fork` can be emulated later in the libc layer with COW.)
- The ELF loader in the kernel is minimal (main segments only, no interpreter, static at first); dynamic linking is a userspace `ld`.
- Startup: `kernel_main` → `init` from the initrd → `svcmgr` services (net, gfx, ... later).
- **Job:** a process group + resource limits (memory, CPU percentage, thread count) → cgroup-like.
- Termination: threads are cancelled, handles closed, the address space freed, and the `exit` event wakes the waiters.

## 6. Syscalls & User Mode

- Entry: aarch64 `svc`, x86_64 `syscall`. The arch stub saves registers into a `SyscallFrame` → common `syscall_dispatch(nr, args)`.
- User memory is accessed only through `user_copy_in/out` and `UserPtr<T>`; page faults in this region are recoverable (fixup table).
- vDSO-like: a shared page for reading time (`clock_gettime` without a syscall).
- Asynchronous I/O ring (`IoRing`): SQ/CQ are shared memory between user and kernel; `IoSubmit(n)` + `WaitMany`.

## 7. Errors/Panic/Log

- `Result<T, Err>`; `panic()` only for unrecoverable situations. On panic the other CPUs are stopped via IPI.
- Log: a lock-free ring buffer, with a worker draining it to serial + fb; direct to serial in early boot.
