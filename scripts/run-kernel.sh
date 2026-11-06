#!/bin/bash
cd "$(dirname "$0")/.."
exec qemu-system-x86_64 \
  -machine q35 -m 512M \
  -drive if=pflash,format=raw,unit=0,readonly=on,file="$(brew --prefix qemu)/share/qemu/edk2-x86_64-code.fd" \
  -cdrom outputs/some.iso \
  -serial stdio "$@"
