# Boot

## 1. Diagnosis of the Current State ("it hangs, it doesn't boot")

Derived by reading the scripts and `bootloader.c` (not verified by running). Causes, in order of likelihood:

1. **The kernel is not in the image.** `make-image.sh` copies only `EFI/BOOT/BOOTX64.EFI` into `efi.img`; `kernel.elf` is copied nowhere. The bootloader opens `\boot\kernel` → `Open` fails → `OPL: Bootloader failed`. Also, when UEFI boots from an El Torito EFI image, `DeviceHandle` is **that FAT image**, not the ISO9660 tree; so even if `iso/boot/kernel` existed it would not have been found. The kernel must be **inside the ESP (FAT)**.
2. **The kernel is an empty `_start() {}`.** No stack is set up and `ret` returns to a random address; moreover, since no console remains after `ExitBootServices`, nothing is visible → it looks "stuck".
3. **The kernel has no output channel.** No serial port/framebuffer; `BOOT_INFO` carries no framebuffer or ACPI RSDP information.
4. **Loading at a fixed address (`0x200000`, `AllocateAddress`).** On real firmware (Fusion/real hardware) that range may be occupied; on aarch64 the RAM base is `0x4000_0000`, so it never works.
5. **Fragile details:** a leftover debug string (`"OPLaasdasds"`), `EFI_STATUS` is not printed on error, `BOOT_INFO` has no signature/version, the memory map is in `EfiLoaderData` but the kernel is not told to "reserve" it, and the QEMU command lacks `-machine/-m/-cpu`.
6. Architecture: only x86_64 is targeted; there is no aarch64 for the macOS ARM host.

> Status (2026-11): items 1–3 and the debug leftover are fixed for x86_64 (kernel in the ESP at `\EFI\OPL\kernel.elf`, serial + framebuffer output, versioned `OplBootInfo` with framebuffer/RSDP/SMBIOS). The bootloader stays in C for now. Items 4 and 6 are still open.

## 2. Target Boot Chain

```
firmware (edk2 / VMware EFI)
   └─ \EFI\BOOT\BOOTAA64.EFI | BOOTX64.EFI   ← opl-boot (same source, two builds)
        1. open serial + GOP console, print "OPL boot"
        2. read \EFI\OPL\kernel.elf and \EFI\OPL\initrd (from the ESP)
        3. load the ELF into physical memory with AllocateAnyPages (relocatable/higher-half)
        4. fill BootInfo: memory map, GOP fb, ACPI RSDP, DTB (if any), SMBIOS, cmdline, initrd, kernel phys/virt
        5. ExitBootServices (retry on the map_key race)
        6. build temporary page tables: kernel higher-half + direct map of all RAM (HHDM)
        7. jump to the kernel entry  (x0 / rdi = &BootInfo)
```

- **Higher-half kernel:** x86_64 `0xFFFF_FFFF_8000_0000`, aarch64 `TTBR1` (`0xFFFF_0000_0000_0000+`). The bootloader builds the page tables; the kernel later creates its own.
- **Single bootloader source:** `src/boot/src/main.rs` (Rust, `uefi` crate) is shared; `src/boot/src/arch/*` holds only (a) the target triple (`*-unknown-uefi`), (b) the page table format, (c) the kernel jump stub.
- ELF loader: `ET_EXEC` higher-half, page-aligned `PT_LOAD` segments, permissions (W^X) from the segment flags.

## 3. BootInfo Contract (`include/boot/bootinfo.h`, C, versioned)

```c
struct OplBootInfo {
    u64 magic;            // 'OPLBOOT\0'
    u32 version, size;
    u64 kernel_phys, kernel_virt, kernel_size;
    u64 hhdm_base;        // direct map offset
    struct OplMemRegion *mmap; u64 mmap_count;   // our own simple type: usable/reserved/acpi/bootloader/kernel/initrd/mmio
    struct OplFramebuffer fb;                    // base, w, h, pitch, format (BGRA/RGBA)
    u64 acpi_rsdp, dtb, smbios;
    u64 initrd_phys, initrd_size;
    const char *cmdline;
    u64 efi_runtime_services;                    // for clock/reset
    u32 arch_flags;
};
```

The bootloader **converts the UEFI memory map into its own simple format**; the kernel knows no EFI types.

## 4. Visibility (Debug) Requirements

- **Early console (HAL):** aarch64 PL011 (QEMU virt `0x0900_0000`), x86_64 16550 (`0x3F8`) + a GOP framebuffer text console on both (so output is visible on screen in VMware when there is no serial).
- The kernel's first job: print a banner + CPU/memory info, then a `wfi`/`hlt` loop. On entry **the stack is set up and bss is cleared**.
- Panic: register dump + backtrace + serial/fb output.

## 5. Image Production

Two outputs, both with **the same ESP contents**:

| Output | Use |
|---|---|
| `build/images/<arch>/disk.img` (GPT + FAT32 ESP) | QEMU (`-drive`), a Fusion disk via `qemu-img convert -O vmdk` |
| `build/images/<arch>/opl.iso` (El Torito EFI, **kernel included** in `efi.img`) | as a "CD/DVD" in Fusion, for quick trials |

ESP contents:

```
EFI/BOOT/BOOTAA64.EFI   (or BOOTX64.EFI)
EFI/OPL/kernel.elf
EFI/OPL/initrd          (tar/cpio: first drivers + init + config)
EFI/OPL/boot.cfg        (cmdline, log level, console selection)
```

Production uses host tools (mtools, xorriso, qemu-img — all installed) as an xmake task.

## 6. Running

- **QEMU aarch64:** `-machine virt,gic-version=3 -cpu max -accel hvf -m 2G -smp 4 -bios edk2-aarch64-code.fd` (+ `-device ramfb` or `virtio-gpu-pci`, `-serial stdio`). Nearly native speed with HVF on a MacBook.
- **QEMU x86_64:** `-machine q35 -cpu max -m 2G -smp 4 -bios edk2-x86_64-code.fd -serial stdio` (TCG on Apple Silicon, slow but works).
- **VMware Fusion (Apple Silicon → aarch64 VM):** in the `.vmx`, `firmware = "efi"`, attach a disk or ISO, serial port → file (`serial0.fileType = "file"`), `svga.present`/3D enabled. x86_64 works the same way on an Intel Mac. An x86_64 guest does **not** run on Fusion on Apple Silicon; x86_64 is tested only with QEMU.
- `xmake` tasks: `xmake run-qemu`, `xmake make-vmx` (generates the `.vmx` from a template).

## 7. Acceptance Criterion (for M0 to count as done)

On QEMU for both architectures and on Fusion for aarch64: `OPL boot → kernel banner → memory map summary` is visible on serial/screen and the machine waits cleanly in idle.
