#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .build/boot

# ── Bootloader (EFI, C for now; Rust `uefi` port planned) ──
clang -target x86_64-unknown-windows \
  -ffreestanding -fno-stack-protector -fshort-wchar \
  -mno-red-zone -mno-stack-arg-probe \
  -I./third_party/uefi -I./include -DEFI_PLATFORM=1 \
  -c src/boot/bootloader.c -o .build/boot/bootloader.o

clang -target x86_64-unknown-windows \
  -nostdlib -fuse-ld=lld-link \
  -Wl,-entry:efi_main \
  -Wl,-subsystem:efi_application \
  .build/boot/bootloader.o -o .build/boot/BOOTX64.EFI

# ── Kernel (Rust, x86_64-unknown-none) ──
PATH="$HOME/.cargo/bin:$PATH" cargo build --release --target-dir .build/cargo
cp .build/cargo/x86_64-unknown-none/release/kernel .build/boot/kernel.elf
