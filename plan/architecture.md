# Architecture

## 1. Layer Diagram

```
┌───────────────────────────── User Mode ─────────────────────────────┐
│ apps │ shell │ compositor │ services (net/gfx/audio) │ drivers(UMD) │
│            libsys (native syscall lib, ntdll-like) + libc-subset    │
├──────────────────────────── syscall boundary ───────────────────────┤
│                         Kernel Mode                                  │
│  Executive:  ps (process/thread)  mm (memory)  ob (object/handle)    │
│              io (I/O manager)  vfs  net  ipc (port/message)  se (security)│
│  Kernel core (ke): scheduler, dispatcher, synchronization, timer, DPC│
│  Driver framework + drivers (bus, storage, net, input, display)      │
│  HAL: platform (ACPI/DT), interrupt controller, timer, PCI/ECAM, DMA │
│  Arch: src/arch/aarch64  |  src/arch/x86_64                          │
├──────────────────────────────────────────────────────────────────────┤
│  opl-boot (UEFI bootloader)  ←  firmware (edk2 / VMware EFI)         │
└──────────────────────────────────────────────────────────────────────┘
```

## 2. Taken from Windows / Taken from Linux

| Area | From Windows | From Linux | Our decision |
|---|---|---|---|
| Kernel objects | Object Manager + **handle table**, ACL/token | — | Every resource (process, thread, event, section, port, file, device) is an `Object`; access is through handles with a rights mask |
| Layering | Executive / Kernel / HAL split | — | As is: `kernel/{ke,ps,mm,ob,io,...}` + `hal/` |
| Interrupt model | IRQL + DPC | softirq/tasklet | 4 levels: `passive`, `dispatch` (DPC/deferred), `device`, `high`. Interrupt handlers stay short; the work goes to the DPC queue |
| Async I/O | IRP + IOCP | io_uring | **Completion ring** (submit/complete queues, shared memory) as the native I/O interface; an IRP-like `IoRequest` inside drivers |
| Driver layering | Device stack (filter/function/bus) | module, devtree | Device node tree, stackable drivers, loadable ELF modules |
| Synchronization | Dispatcher objects (event, mutex, semaphore, wait-multiple) | **futex**, RCU, per-CPU | Futex-based fast path in userspace, dispatcher objects + `wait_multiple` in the kernel; RCU for read-mostly structures |
| Scheduler | 32 priorities, real-time class | CFS/**EEVDF**, per-CPU runqueue | Per-CPU runqueue + 2 classes: `realtime` (fixed priority) and `fair` (EEVDF-like); work stealing; CPU affinity |
| Memory | Section object, working set, VAD | buddy + **slab**, page cache, per-CPU cache, COW | Buddy PMM + slab + per-CPU magazine; VMA tree; section = shared memory object; unified page cache |
| IPC | ALPC (port) | pipe/socket/eventfd | `Port` object (message + handle transfer), shared memory section, futex |
| FS | IRP-based FS driver | **VFS + dentry cache** | VFS (dentry/inode cache) + FS drivers as modules; mount = device node |
| Networking | NDIS/WinSock | socket layer, skb | `NetBuf` + netdev interface + sockets; NICs are drivers |
| Isolation | Job object | cgroups + namespaces | `Job` object (limits + grouping), namespaces later |
| Config | Registry | sysfs/devicetree | No registry; a simple hierarchical **config store** (initrd/disk file) |
| Graphics | WDDM: UMD/KMD, VidMM, VidSch | DRM/KMS + GBM | Only a thin KMD interface in the kernel; the rest is userspace (`graphics.md`) |
| Debug | WinDbg, ETW | ftrace, printk | Serial log ring, panic + backtrace, trace points (later) |

## 3. Kernel Subsystems (responsibility summary)

- **ke (Kernel core):** thread dispatcher, scheduler, wait/wake, spinlock/mutex/event/semaphore, kernel timer, DPC, per-CPU data. *(C++, C with arch)*
- **ps (Process):** process/thread/job objects, ELF loader, process lifecycle, asynchronous notification in the style of "exception port + APC" instead of signals. *(Rust)* *(C++)*
- **mm (Memory):** PMM (buddy), VMM (address space, VMA, page fault), slab/heap, section objects, page cache backend. *(C++; MMU/TLB parts in C)*
- **ob (Objects):** reference counting, handle table, namespace, rights checks. *(Rust)*
- **io (I/O manager):** `IoRequest`, device node tree, completion ring, driver loading. *(Rust; drivers are Rust too)*
- **vfs:** mount, path resolution, dentry/inode cache, file object. *(Rust)*
- **net:** NetBuf, netdev, ARP/IP/UDP/TCP, socket object. *(Rust; a mature smoltcp-like crate may be used at first)*
- **ipc:** port, message, shared memory. *(Rust)*
- **se (Security):** token, rights, capability/sandbox later. *(Rust)*
- **hal:** platform abstraction (ACPI/DTB), interrupt controller (GICv3 / APIC), timer (ARM generic timer / LAPIC+TSC), PCI ECAM, DMA/IOMMU. *(low-level C + C++ logic)*
- **arch:** CPU features, context switch, MMU setup, exception vectors, syscall entry, SMP bring-up. *(C + asm)*

## 4. The Kernel's Main Boundary: Syscalls

- A small, handle-based surface (target ~100 calls): `ObjClose`, `ProcCreate`, `ThreadCreate`, `MemMap`, `PortSend`, `IoSubmit` (ring), `WaitMany`, `FutexWait/Wake`...
- Userspace `libsys` wraps these syscalls; libc is built on top. Syscall numbers and structs live under `include/abi/` and are **architecture independent**.
- aarch64: `svc #0`; x86_64: `syscall/sysret`. Entry code lives in arch, the dispatcher is shared.

## 5. Security Foundations (from day one)

W^X, guard pages, user/kernel separation (PAN/PXN, SMAP/SMEP), KASLR (later), stack canary, `copy_from_user`-style safe helpers for user pointer access, handle rights.

## 6. What We Are Not Doing (for now)

Registry, COM, Win32 compatibility, GUI/graphics in the kernel, JIT in the kernel, an extra POSIX layer. All of these can be added later as userspace layers.
