# Languages & ABI

**Priority: Rust = xlang > C++ > C.** xlang counts as equal to Rust. For new code the default is Rust or xlang; C++ only where Rust is poor (intrusive/atomic-heavy kernel structures), C only where there is no other option. Reason: build speed + compiler-enforced safety (memory/data-race bugs are caught at compile time).

## Division of Work

| Language | Where | Note |
|---|---|---|
| **Rust** | bootloader (`uefi` crate), drivers, VFS and file systems, network stack, `kernel/{ps,ob,io,ipc,se,syscall}`, `kernel_main`, userspace (`libsys`, `svcmgr`, `init`, `shell`), `gfx/runtime`, `gfx/shaderc`, `gfx/display`, host tools (`tools/`) | `no_std` + `alloc`, `panic=abort`, `unsafe` only at the MMIO/FFI/arch boundary with a `// SAFETY:` on every block |
| **C++26** | `kernel/ke` (scheduler, dispatcher, sync), `kernel/mm` (PMM, slab, VMA), `hal` logic, `kstd`, rasterizer hot loops (SIMD) | Freestanding, **no exceptions/RTTI**, `-std=c++2c`; exposed to Rust through thin `extern "C"` wrappers |
| **C** | `arch/*` (exception/syscall entry, MMU/TLB, GIC/APIC, SMP), MMIO accessors, public ABI headers (`include/abi`, `include/odk`, `include/uapi`, `gfx/include`), libc core such as `memcpy/memset` | The C tree is kept small; before adding a new C file ask "can't this be Rust/C++?" |
| **asm (.S)** | context switch, exception/syscall entry, boot stub | in arch directories; in the Rust x86_64 slice these are single-instruction `asm!` blocks and one `naked_asm!` function, all under `src/arch/x86_64/` |
| **xlang** | **Everywhere** (equivalent to Rust): drivers, ps/ob/io, ke/mm, the non-C parts of arch, userspace, gfx | Goes anywhere Rust can; the compiler work list is below |

> The old rule in `CLAUDE.md` ("C low level, C++ process/memory/thread, Rust wherever you like") is superseded by this file. `CLAUDE.md` has since been updated to match this table.

## Language Selection Rule (decision tree)

1. Very close to hardware/arch, a few lines of asm + registers (trap entry, MMU, GIC)? → **C** (+ `.S`).
2. Hot path, intrusive list/tree, per-CPU, atomic/lock-free structure, scheduler/PMM/slab? → **C++26**.
3. Everything else → **Rust or xlang** (the two are equal; xlang may be preferred if the xlang compiler is ready for that target, otherwise Rust).
4. If something needs two languages: the interface lives in a C ABI header and both sides are fed from that header.

## Rust Rules

- A single `Cargo` workspace (`src/rust/`): `ksys` (a safe wrapper over the kernel API), `kobj`, `kio`, `kvfs`, `knet`, driver crates, `opl-boot`. xmake invokes the workspace with `cargo`/`rustc` and links the resulting `staticlib` into the kernel. *(In the current x86_64 slice the crate is simply the root `Cargo.toml`, built by `scripts/build-kernel.sh`.)*
- Targets: `aarch64-unknown-none-softfloat`, `x86_64-unknown-none`; bootloader `aarch64-unknown-uefi`, `x86_64-unknown-uefi`. The nightly is pinned with `rust-toolchain.toml`, `build-std = [core, alloc, compiler_builtins]`.
- Kernel ↔ Rust boundary: `#[repr(C)]` + C ABI; headers have a single source in `include/` (converted to Rust with bindgen; the direction is never reversed).
- The allocator is bound to the kernel slab/heap; allocation in ISR context is forbidden (a `#[must_not_suspend]`-like discipline + lint).
- Errors: `Result<T, Errno>`; `panic!` only for the unrecoverable.

## C++26 Rules (kernel)

- No `new`; `kstd::make<T>(cache, ...)` + RAII; `noexcept` by default; `Result<T,E>` (`std::expected`).
- `constexpr/consteval`, `concepts`, `span`, `string_view`-like types; `std::atomic` with explicit `memory_order`.
- Contracts/static reflection are not in clang → do not use them; gate with feature-test macros.
- Global constructors are controlled; no static-initialization-order dependencies.

## ABI Principles

1. Everything crossing a language boundary is **C ABI** + a versioned struct (`size`/`version`).
2. The C++ internal ABI is unstable; Rust's ABI is never used (`extern "C"` is mandatory).
3. The syscall ABI is documented per architecture (aarch64: x8=nr, x0–x5; x86_64: rax=nr, rdi,rsi,rdx,r10,r8,r9).

## Bringing xlang into the Kernel

`../xlang` (LLVM based, a compiler written in C++) was examined. What is useful for the kernel: `--emit=obj`, `--target <triple>`, `--no-std`, `@repr(c)`, `export(c)`, `declare` (external symbol), `@syscall(n, ...)`. **Not there yet** (from its own `known-limitations.md`): no bare-metal/`*-none-elf` target and no entry stub (it is Linux/macOS/Windows oriented), and `export(c)` does not pass structs by value with the C ABI (only pointers, scalars and `@repr(c)` structs behind a pointer are safe).

**Decision: xlang will be used in the kernel tree.** The requirements below are not "will it be used?" but the **work list** to be added to the xlang compiler (tracked in the xlang repo); the kernel side opens up as they become ready:

| # | Requirement | Status in xlang |
|---|---|---|
| X1 | Produce `--emit=obj --no-std` with `--target aarch64-unknown-none-elf` / `x86_64-unknown-none-elf` | **`none-elf` target missing** (`unsupported target`). Workaround: `--target x86_64-unknown-linux-gnu --emit=obj --no-std` → no undefined symbols, no red-zone use; `build.rs` compiles it this way (`src/kernel/mm/heap.xl` and `pmm.xl` work) |
| X2 | Freestanding: no stdlib and no entry stub, leaving calls such as `memcpy/memset` to the kernel | `--no-std` exists; the hooks it calls (`panic/alloc/free/unwind`) must be supplied by the kernel |
| X3 | Pointer/scalar/`@repr(c)` boundary with the C ABI | exists (struct-by-value forbidden) |
| X4 | `volatile`/MMIO, atomic access, inline asm or calling an asm stub via `declare` | **to be verified** |
| X5 | Panic/allocator hooks must be bindable to kernel providers | `panic/alloc/free` roots exist, the kernel side is to be written |

**Phases** (xlang entering everywhere depends on this work list; the order follows compiler maturity only)
- **X-A (immediate, no risk):** add an `xlang` rule to xmake, build a small xlang program under `tools/` or in userspace (host target).
- **X-B:** open X1–X5 as work items in the xlang repo (`none-elf` target + freestanding mode). Try it with a single driver module in the kernel such as `src/drivers/xl-hello/`. *(Partly done: heap and PMM are already xlang.)*
- **X-C:** if successful, extend to drivers, then `ps/ob/io`, and finally `ke/mm` and the parts of arch that can replace C. No restriction; the only limit is whether the compiler generates that code (atomics, inline asm, volatile, ABI) correctly.
