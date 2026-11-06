# Build & Test

## 1. Toolchain (verified on this machine)

| Tool | Status |
|---|---|
| clang/clang++ 23.1 (Homebrew LLVM) | present, `-std=c++2c` works |
| lld (`ld.lld`, `lld-link`) | present (`/opt/homebrew/bin`) |
| xmake 3.1 | present |
| QEMU aarch64 + x86_64, edk2 firmware | present (`/opt/homebrew/share/qemu/edk2-*-code.fd`) |
| mtools, xorriso, qemu-img | present |
| VMware Fusion | installed |
| rustc 1.98 (Homebrew) | present but **no bare-metal target** → `rustup` + nightly + `rust-src` (`build-std=core,alloc`) is required; targets `aarch64-unknown-none-softfloat`, `x86_64-unknown-none` |
| xlang (`xl`) | present globally (`~/.local/bin/xl`); no `none-elf` target yet, see `languages.md` X1 |

C++26 note: **contracts** (`__cpp_contracts`) are not yet defined in clang; we use what is supported, such as `[[assume]]`, `std::expected`, `constexpr` improvements, pack indexing and `#embed`. Static reflection is ignored. When a feature is needed it is gated with a feature-test macro.

## 1.1 Pinning the Toolchain (xmake)

Apple's `/usr/bin/clang` is not suitable for bare-metal/aarch64-none and lld; **Homebrew LLVM** is used. The root `xmake.lua` pins the path and fails with a clear error if it is missing:

```lua
-- xmake/toolchains.lua (draft)
local llvm = "/opt/homebrew/opt/llvm/bin"
toolchain("opl-llvm")
    set_kind("standalone")
    set_toolset("cc",  llvm .. "/clang")
    set_toolset("cxx", llvm .. "/clang++")
    set_toolset("ld",  llvm .. "/clang++")
    set_toolset("ar",  llvm .. "/llvm-ar")
    set_toolset("as",  llvm .. "/clang")
    -- lld: -fuse-ld=lld (ld.lld, lld-link /opt/homebrew/bin)
toolchain_end()
set_toolchains("opl-llvm")
```

| Language | xmake equivalent |
|---|---|
| C / C++ / asm | `opl-llvm` toolchain, target `--target=<arch>-unknown-none-elf`, `-std=c23` / `-std=c++2c` |
| Rust | a custom `rule("rust.kernel")`: `rustc`/`cargo` (rustup nightly, `rust-toolchain.toml`), `--target`, `staticlib`, `panic=abort`; `*-unknown-uefi` for the bootloader |
| xlang | a custom `rule("xlang")`: `on_build_file` → `xlang build x.xl --emit=obj --no-std --target <triple> -o x.o`; the xlang path comes from the `XLANG` env or `../xlang/outputs/xlang`. Host target only until gates X1–X5 (`languages.md`) are passed |
| Linker | `ld.lld` (ELF kernel/modules), `lld-link` (if PE is used) |

xlang's own `xmake/toolchains.lua` context (package based) is taken as an example, but the path is kept fixed in the kernel. Note: if "my clang" refers to something other than Homebrew LLVM (for example a custom-built clang), the path is changed with the `xmake f --llvm=<dir>` option.

## 2. xmake Layout

```
xmake.lua                       # options: arch(aarch64|x86_64), mode(debug|release), toolchain(clang)
src/boot/xmake.lua              # target "opl-boot": Rust, *-unknown-uefi (rustc produces the PE/EFI)
src/arch/xmake.lua              # picks the right directory by arch
src/kernel/xmake.lua            # target "kernel": ELF, -ffreestanding -fno-exceptions -fno-rtti -mno-red-zone(x86), -mgeneral-regs-only
src/drivers/xmake.lua           # Rust (kernel rule); optional C/C++/xlang modules
src/user/xmake.lua              # libsys, libc, init...
gfx/xmake.lua                   # graphics stack (separate target group)
tests/xmake.lua                 # host unit tests
tools/xmake.lua                 # host tools (mkimage, mkinitrd)
```

- One command: `xmake f -a aarch64 -m debug && xmake` ; `xmake f -a x86_64 && xmake`.
- Output: `build/<arch>/<mode>/...`; images in `build/images/<arch>/`.
- **Rust**: a staticlib through the xmake `rust` rule (`--crate-type staticlib`, `panic=abort`) linked into the kernel; or a small `Cargo.toml` workspace invoked from xmake.
- Common flags in one place: `-ffreestanding -fno-exceptions -fno-rtti -fno-stack-protector (canary later) -fno-pic/-fPIE -nostdlib -Wall -Wextra -Werror` + a host test build with sanitizers.
- Cross-target: `--target=aarch64-unknown-none-elf` / `x86_64-unknown-none-elf`; `*-unknown-windows` for boot.

### xmake tasks (instead of scripts/*.sh)

| Task | Job |
|---|---|
| `xmake image` | ESP contents → `disk.img` (GPT+FAT32) and `opl.iso` |
| `xmake run-qemu` | starts QEMU for the selected arch with the right flags (`--gdb`, `--no-gfx`, `--accel`) |
| `xmake make-vmx` | produces the `.vmx` + `vmdk` for Fusion |
| `xmake test` | host unit + QEMU integration tests |
| `xmake symbolize` | panic backtrace → line (llvm-symbolizer) |

## 3. Run Matrix

| Environment | aarch64 | x86_64 |
|---|---|---|
| QEMU (Apple Silicon) | `-accel hvf` (fast) | TCG (slow) |
| VMware Fusion (Apple Silicon) | **yes** | no |
| VMware Fusion (Intel Mac) | no | yes |

CI/local rule: on every PR both architectures **build**; an aarch64 QEMU+HVF smoke test runs; an x86_64 QEMU smoke test (TCG) runs.

## 4. Test Strategy

1. **Host unit tests:** `kstd` (containers), buddy/slab logic, the VMA tree, scheduler queue logic — built on the host (macOS), fast.
2. **QEMU integration:** a `ktest` framework inside the kernel; results over the serial port; exit code: `isa-debug-exit` on x86, semihosting/PSCI `SYSTEM_OFF` on aarch64. The xmake test looks for `ALL TESTS PASSED` in the serial output, with a timeout.
3. **Boot smoke:** are `OPL boot` + the kernel banner visible (both archs).
4. **Fusion manual:** a checklist at the end of each milestone (image on screen, serial log file).
5. **Sanitizer/fuzz:** fuzzing on the host for the ELF loader, the network packet parser and filesystem parsers.
6. **Debug:** the QEMU gdbstub with `-s -S`, `lldb`/`gdb` command files in `tools/`.

## 5. Acceptance Criterion (for M0 to count as done)

On QEMU for both architectures and on Fusion for aarch64: `OPL boot → kernel banner → memory map summary` is visible on serial/screen and the machine waits cleanly in idle.
