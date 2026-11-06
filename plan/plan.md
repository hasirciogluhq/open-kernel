# open-kernel — Master Plan

> Status: **draft v0.1** (2026-11-06). The files in the old docs/ directory are another AI's notes; they were used as a source of ideas and are not binding.

## Vision

A layered, object/handle-based kernel in the style of Windows (NT), focused on asynchronous I/O, combined with Linux's modularity, memory management (slab/RCU/per-CPU) and scheduler ideas. **The graphics system lives outside the kernel** as a separate stack.

## Fixed Decisions

| Topic | Decision |
|---|---|
| Primary architecture | **aarch64** (Apple Silicon host: QEMU+HVF and VMware Fusion ARM) |
| Parallel architecture | **x86_64** (QEMU; Fusion runs it only on Intel Macs) |
| Firmware | UEFI (our own bootloader, one source for both architectures) |
| Languages | **Priority: Rust = xlang > C++ > C.** Rust: bootloader, drivers, VFS/FS, networking, ps/ob/io/ipc/se, userspace services, gfx runtime. C++26: `ke`, `mm`, `hal` logic, rasterizer inner loops. C: `arch/*`, MMIO/trap, ABI headers, asm glue. xlang: equivalent to Rust, usable anywhere (compiler work list in `languages.md`) |
| C++ standard | `-std=c++2c` (C++26), freestanding, no exceptions/RTTI |
| Compiler | Homebrew **LLVM clang/lld** (not Apple clang), Rust `rustc`, `xlang` — all as xmake toolchains/rules (`build-and-test.md`) |
| Build | **xmake** (scripts/*.sh will go away) |
| Kernel type | Hybrid: monolithic kernel + loadable drivers, with a clear kernel-mode/user-mode split |
| Graphics | Not in the kernel. The kernel only provides a thin "GPU/display device class" interface |
| Test targets | QEMU (aarch64 `virt`, x86_64 `q35`) and VMware Fusion (UEFI VM) |

## Document Map

| File | Contents |
|---|---|
| `architecture.md` | Layers, what is taken from Windows and Linux, kernel subsystems |
| `folder-structure.md` | Directory layout and dependency rules |
| `boot.md` | Bootloader, boot protocol, **diagnosis of the current boot problem**, ISO/disk image |
| `kernel-skeleton.md` | **Checklist of all skeleton features**: kernel entry/exit, context switch, syscall families, epoll/io_uring equivalent, growth rules |
| `kernel-core.md` | CPU, interrupts, threads, scheduler, synchronization, process, syscalls |
| `memory.md` | PMM, VMM, slab/heap, page cache |
| `drivers-io-fs-net.md` | Driver model, I/O manager, VFS, networking |
| `graphics.md` | Separate graphics stack: API → runtime/router → UMD → KMD, CPU rasterizer, 3D paths |
| `build-and-test.md` | xmake, toolchain, running on QEMU/Fusion, test strategy |
| `languages.md` | Boundaries and ABI rules for C / C++26 / Rust / xlang |
| `roadmap.md` | Milestones and the first sprint |

## Design Principles

1. **Simple surface, single responsibility.** Each layer knows the layer below only through its interface.
2. **Architecture code lives only under `src/arch/<arch>/`.** No `#ifdef __aarch64__` in upper layers.
3. **Visibility first:** no milestone counts as "done" without a serial port + framebuffer console (the lack of it is one of the reasons the current boot appears stuck).
4. **The kernel provides mechanism, policy lives in userspace** (graphics, compositor, service management).
5. **New code is built for both architectures first**, then run.
6. **Asynchronous I/O is first class** (completion ring); blocking calls are wrappers on top of it.

## Open Questions (defaults written; will change if you object)

- **Executable format:** default is **ELF**. "Windows-like" means architecture/API style for us; PE/Win32 compatibility comes much later, as an optional userspace layer.
- **Project/kernel name:** I keep the "OPL" prefix from the bootloader output (`opl-boot`, `OPL_` macros).
- **Rust toolchain:** Homebrew's Rust has no bare-metal target; `rustup` + nightly (`build-std`) is required (`build-and-test.md`).
