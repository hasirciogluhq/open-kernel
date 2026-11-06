# Roadmap

For each milestone: **aarch64 + x86_64 build**, run on QEMU; aarch64 is additionally verified on Fusion. No time estimates are given (it is a free-time project); the order matters.

| M | Title | Output / Acceptance criterion |
|---|---|---|
| **M0** | Repo + build + boot fix | Move to the `plan/folder-structure.md` layout; xmake skeleton; `opl-boot` (both archs) finds the kernel **inside the ESP**; the kernel writes a banner to serial + fb and enters idle; `xmake image`, `run-qemu`, `make-vmx` work. *Diagnosis: `boot.md` §1.* |
| **M1** | Bootloader complete | `OplBootInfo` (mmap, fb, RSDP/DTB, initrd, cmdline), higher-half + HHDM switch, `boot.cfg`, a loader that prints error codes |
| **M2** | Arch foundation | exception/IRQ vectors, GICv3/APIC, timer tick, panic + backtrace, early console (HAL), per-CPU |
| **M3** | Memory | PMM (buddy) → slab/`kmalloc` → own page tables → VMA + page fault; host unit tests |
| **M4** | Thread & scheduler | context switch, single-CPU scheduler, sync primitives, timers/wait, then SMP bring-up + per-CPU runqueue |
| **M5** | Process & user mode | syscall entry, ELF loader, handle table, `init` in userspace printing "hello", `libsys`, futex |
| **M6** | Driver framework + storage | ODK, device tree, PCI/virtio, `virtio-blk`, block layer, VFS + `tarfs`/`fat32`, page cache, I/O ring |
| **M7** | Networking | `netdev`/NetBuf, `virtio-net`, TCP/IP stack, socket object, DHCP + ping + TCP echo |
| **G0–G1** | Graphics foundation | `gop-fb` KMD, BufferObject/Fence/Scanout syscalls, a triangle on screen with `gfx/soft` |
| **X** | xlang bare-metal | a `*-none-elf` target + freestanding mode in xlang (languages.md X1–X5); after M5 the first xlang driver module is built and loaded |
| **M8** | Hardening | loadable modules, W^X/KASLR, lockdep, fuzz, Job limits, OOM |
| **G2–G5** | Graphics progress | CPU 3D rasterizer (SIMD+multi-thread) → virtio-gpu 3D → VMware SVGA3 → compositor |
| **M9+** | Advanced | USB, NVMe/AHCI, native FS, power management, user-mode driver framework, xlang integration, (optional) PE/Win32 compatibility layer |

> Progress note (x86_64 only, QEMU): M0 is met and parts of M2–M4 exist early: GDT/IDT/TSS, PIT tick, backtrace, PMM + heap (xlang), single-CPU preemptive threads (spawn/sleep/suspend/resume/terminate/join), ACPI/SMBIOS/PCI discovery. Not yet: higher-half/own page tables, LAPIC/SMP, aarch64.

## First Sprint (M0 task list)

1. Approve `plan/` → close the open questions (ELF vs PE, project name).
2. Set up the directory skeleton, move the existing files (`folder-structure.md` table).
3. Root `xmake.lua`: `arch`/`mode` options, clang toolchain, common flags.
4. `src/arch/aarch64/start.S` + `x86_64/start.S`: set up the stack, clear bss, call `kernel_main(BootInfo*)`; PL011/16550 `putc`.
5. `opl-boot`: read the kernel as `\EFI\OPL\kernel.elf`, `AllocateAnyPages`, print the status code on error, aarch64 PE target (`BOOTAA64.EFI`).
6. `xmake image` (**kernel included** in the ESP) + `run-qemu`; then `make-vmx` and a Fusion trial.
7. First host test: a small `kstd` container + `xmake test`.

## Risks

| Risk | Mitigation |
|---|---|
| x86_64 does not run on Fusion/Apple Silicon | x86_64 only on QEMU; accept this from the start |
| virtio-gpu 3D (virgl) is limited on macOS | CPU rasterizer is the primary path; SVGA3 acceleration on Fusion |
| Rust bare-metal toolchain setup | rustup nightly, pinned with `rust-toolchain.toml` |
| C++26 features missing in clang (contracts) | gate with feature tests; core `kstd` uses its own assert |
| Scope creep | finish each milestone only against its acceptance criterion; graphics/networking do not grow the core |
