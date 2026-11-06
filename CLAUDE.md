Languages: Rust = xlang > C++26 > C (priority order). xlang is treated as equal to Rust. Details: plan/languages.md

- Rust (or xlang): bootloader, drivers, VFS/FS, net, ps/ob/io/ipc/se, userspace, gfx runtime. Default for new code.
- C++26 (clang, -std=c++2c, freestanding, no exceptions/RTTI): kernel ke/mm, HAL logic, kstd, rasterizer hot loops.
- C: only arch/* low-level (trap, MMU, GIC/APIC), MMIO, public ABI headers, asm glue.
- xlang (../xlang, own LLVM-based language): same tier as Rust, allowed anywhere in the tree (incl. ke/mm/arch). Needs bare-metal target + freestanding mode in the xlang compiler; track as work items in ../xlang. See plan/languages.md.

Build: xmake, Homebrew LLVM clang/lld (not Apple clang). Plans live in plan/.
