#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

rm -rf .build/iso .build/efi.img
mkdir -p .build/iso/boot outputs

dd if=/dev/zero of=.build/efi.img bs=1m count=64 2>/dev/null
mformat -i .build/efi.img -F ::
mmd -i .build/efi.img ::/EFI ::/EFI/BOOT ::/EFI/OPL
mcopy -i .build/efi.img .build/boot/BOOTX64.EFI ::/EFI/BOOT/
mcopy -i .build/efi.img .build/boot/kernel.elf ::/EFI/OPL/kernel.elf
cp .build/efi.img .build/iso/efi.img

xorriso -as mkisofs \
  -R -J -V OPL \
  -o outputs/some.iso \
  -eltorito-alt-boot \
  -e efi.img \
  -no-emul-boot \
  .build/iso
