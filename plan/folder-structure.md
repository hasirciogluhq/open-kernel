# Folder Structure

Differences from the old `docs/the folder structure tapology.md`: `arch` is not at the root but under **`src/arch`**, graphics is **outside the kernel tree**, and every component has its own `xmake.lua`.

```
open-kernel/
├── xmake.lua                    # root: toolchain, arch/mode options, includes the subdirectories
├── CLAUDE.md  readme.md
├── plan/                        # these documents
├── docs/                        # reference notes (old), to be reorganized later
│
├── include/                     # PUBLIC headers (kernel↔driver↔userspace contracts)
│   ├── abi/                     # syscall numbers, structs, error codes (C, architecture independent)
│   ├── boot/                    # BootInfo (bootloader ↔ kernel contract)
│   ├── odk/                     # Open Driver Kit: the API drivers see (C ABI)
│   └── uapi/                    # kernel headers visible to userspace
│
├── src/
│   ├── boot/                    # opl-boot: UEFI bootloader, **Rust** (uefi crate, target *-unknown-uefi)
│   │   ├── Cargo.toml  src/{main,elf,paging,gop,acpi}.rs
│   │   └── src/arch/{aarch64,x86_64}.rs   # page tables + kernel jump (core::arch::asm!)
│   │
│   ├── arch/                    # architecture code ONLY
│   │   ├── aarch64/
│   │   │   ├── start.S  exception.S  context.S  syscall.S
│   │   │   ├── cpu.c  mmu.c  gic.c  timer.c  smp.c  psci.c      (C)
│   │   │   └── linker.ld
│   │   ├── x86_64/
│   │   │   ├── start.S  idt.S  context.S  syscall.S
│   │   │   ├── cpu.c  paging.c  apic.c  hpet_tsc.c  smp.c  gdt.c  (C)
│   │   │   └── linker.ld
│   │   └── common/              # helpers shared between archs (atomics, bitops)
│   │
│   ├── hal/                     # platform abstraction
│   │   ├── acpi/  dtb/  irq/  timer/  pci/  dma/  console/(serial+fb text)
│   │
│   ├── kernel/                  # the kernel (arch independent)
│   │   ├── ke/                  # scheduler, dispatcher, sync, timer, dpc        (C++)
│   │   ├── ps/                  # process, thread, job, elf loader               (Rust)
│   │   ├── mm/                  # pmm, vmm, slab, heap, section, pagecache       (C++)
│   │   ├── ob/                  # object manager, handle table                   (Rust)
│   │   ├── io/                  # io manager, device tree, ring, driver loader   (Rust)
│   │   ├── ipc/                 # port, shared memory                            (Rust)
│   │   ├── se/                  # token/rights                                   (Rust)
│   │   ├── vfs/                 # VFS core                                       (Rust)
│   │   ├── net/                 # netbuf, stack, socket                          (Rust)
│   │   ├── syscall/             # dispatcher + table
│   │   ├── panic/  log/         # panic, serial log ring
│   │   └── main.rs              # kernel_main (Rust), FFI with ke/mm C++
│   │
│   ├── kstd/                    # in-kernel freestanding "standard library" (Rust `ksys` crate + a small C++26 `kstd` for ke/mm)
│   │   # span, expected, intrusive_list, static_vector, string_view, format, bitmap, rbtree...
│   │
│   ├── drivers/                 # loadable/static drivers (Rust; C/C++/xlang if needed)
│   │   ├── bus/{pci,virtio,usb(later)}
│   │   ├── storage/{virtio-blk,nvme,ahci}
│   │   ├── fs/{fat32,ext2,tarfs(initrd)}
│   │   ├── net/{virtio-net,e1000}
│   │   ├── input/{virtio-input,ps2,usb-hid}
│   │   ├── serial/{pl011,uart16550}
│   │   └── display/{gop-fb,virtio-gpu,vmware-svga}     # KMD side only
│   │
│   └── user/                    # userspace (part of the kernel tree but built separately)
│       ├── libsys/              # native syscall library
│       ├── libc/                # minimal libc
│       ├── init/  svcmgr/  shell/
│       └── tools/
│
├── gfx/                         # *** SEPARATE GRAPHICS STACK (independent of the kernel, depends only on include/abi+uapi) ***
│   ├── xmake.lua
│   ├── include/ogfx/            # public graphics API headers
│   ├── runtime/                 # loader/router: find device, pick UMD, resource management
│   ├── umd/{soft,virtio,vmware,amd,nvidia,intel}/   # user-mode drivers (ICD)
│   ├── shaderc/                 # shader IR (SPIR-V) compiler/interpreter
│   ├── display/                 # compositor / display server
│   └── tests/
│
├── tests/{unit,integration,hardware}/
├── tools/                       # host tools (image builder, mkinitrd, symbolizer)
├── scripts/                     # thin wrappers (the real work is in xmake tasks)
├── third_party/                 # efi headers (the current efi/ moves here), smoltcp etc.
└── build/                       # output (a single place instead of .build), outputs/ → build/images/
```

> Note (2026-11): the x86_64 slice already follows this layout in a mixed-language way — Rust, C and xlang files share the same directories (for example `src/kernel/mm/` holds `heap.xl`, `pmm.xl` and the Rust glue), and the root `Cargo.toml` + `build.rs` compile `.xl`/`.c` files found under `src/{kernel,hal,arch,drivers}` into the kernel. `efi/` has moved to `third_party/uefi/`.

## Dependency Rules (one direction, downward)

```
user/gfx  →  include/{abi,uapi}                       (never includes kernel code)
kernel/*  →  kstd, hal, include/*   (inside kernel/: ke ← mm ← ob ← ps ← io ← vfs/net/ipc)
drivers   →  include/odk  (do not see kernel internal headers; only the ODK API)
hal       →  arch (only through the arch/<x>/ interface, include/arch.h)
arch      →  no upper layer
```

- `include/odk` and `include/abi` are a **stable C ABI**; Rust (`bindgen`) and C++ are fed from the same C headers; the xlang side binds with `@repr(c)` + `declare`.
- `gfx/` is kept so that it can be split into its own repo: its only tie to the kernel is syscalls + `include/uapi`.

## Migration Plan (current → new)

| Current | New |
|---|---|
| `src/boot/bootloader.c` | `src/boot/src/main.rs` (rewritten in Rust, `uefi` crate) — *decision: stays C for now* |
| `src/kernel.c` | `src/kernel/main.rs` + `src/arch/*/start.S` — *done for x86_64 (Rust, no `.S`)* |
| `src/memory/heap.c`, `include/memory/*` | `src/kernel/mm/` — *done: now `heap.xl` (xlang)* |
| `efi/` | `third_party/uefi/` — *done* |
| `scripts/kernel.ld` | `src/arch/*/linker.ld` — *done for x86_64* |
| `scripts/*.sh` | xmake tasks (`build-and-test.md`) |
| `.build/`, `outputs/` | `build/` |
