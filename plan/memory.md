# Memory Management

Languages: the PMM/slab/VMM core is C++26 (intrusive structures, atomic-heavy), MMU/TLB work lives in `src/arch/*` (C), the page cache and section/handle side is Rust.

## 1. Layers

```
kmalloc / typed caches (slab)      ← in-kernel objects
page-level allocator (buddy)       ← pages/blocks
physical memory map (BootInfo)     ← regions from the bootloader
```

- **PMM:** buddy allocator (order 0..10), a "zone" structure ready for NUMA (a single zone at first; a DMA32 zone for drivers). Bitmap only in early boot. A lock-free fast path through a per-CPU page cache.
- **Slab:** size classes (`kmalloc` 16B..8K) + typed caches (`Cache<Thread>`, `Cache<Vma>`). A per-CPU "magazine" layer. Debug build: redzone/poison.
- **Heap (large):** `vmalloc`-like — virtually contiguous, physically scattered; taken from the kernel VA region.
- **HHDM:** all physical RAM is direct-mapped in the kernel address space (`hhdm_base + phys`). Physical↔virtual conversion is O(1).

## 2. Virtual Memory

- **AddressSpace** object: upper half is kernel (shared), lower half is user. `TTBR0/TTBR1` (aarch64), `CR3` (x86_64).
- **Vma** (red-black tree or maple-tree-like): `[start,end)`, permissions (R/W/X), backing (anonymous / section / file), flags (shared, cow, stack-guard, no-fork).
- **Page fault path:** find VMA → permission check → anonymous: zero page; file: page cache; COW: copy; on failure an exception to the user, or in the kernel the fixup table or a panic.
- **Section object:** a handle-based object for shared memory / file mappings (Windows section ≈ Linux `memfd`+`mmap`). Graphics buffers, IPC and the loader are built on it.
- **W^X enforced:** a page cannot be both writable and executable at the same time (transitions for JIT go through a separate API).
- **ASLR:** user stack/mmap/heap; KASLR later.
- **Overcommit:** strict accounting (commit limit) at first, OOM killer later (with job limits).

## 3. Page Cache & File Memory

- A single, unified **page cache**: VFS read/write and `mmap` share the same pages. Key `(inode, page_index)`.
- Dirty tracking + a writeback worker thread; `fsync` support.
- Reclaim: a simple clock/LRU 2-list (active/inactive); at first only clean page cache pages are dropped, no swap.

## 4. DMA & MMIO

- `DmaBuffer` (physically contiguous, cache-coherent or with sync calls), an IOMMU-ready interface (SMMU/VT-d later).
- MMIO mapping: `ioremap(phys, size, attrs)` → device/uncached attributes (aarch64 `MAIR` Device-nGnRE; x86_64 PAT/UC).
- Available to drivers only through ODK: `odk_dma_alloc`, `odk_mmio_map`.

## 5. Multi-core Layout

- TLB shootdown: aarch64 `tlbi ... is` (broadcast), x86_64 IPI. Batched invalidation.
- Per-CPU kernel stack (16 KiB + guard page), separate IRQ stack.

## 6. Kernel Address Space Map (draft)

| Region | x86_64 | aarch64 |
|---|---|---|
| User | `0x0000_0000_0000_0000 – 0x0000_7FFF_FFFF_FFFF` | `0x0000_0000_0000_0000 – 0x0000_FFFF_FFFF_FFFF` (TTBR0) |
| HHDM | `0xFFFF_8000_0000_0000 +` | `0xFFFF_0000_0000_0000 +` |
| vmalloc / ioremap | `0xFFFF_C000_0000_0000 +` | `0xFFFF_8000_0000_0000 +` |
| Kernel image | `0xFFFF_FFFF_8000_0000 +` | `0xFFFF_FFFF_8000_0000 +` |

## 7. Milestone Order

1. BootInfo memory map → PMM (buddy) → `kmalloc` (slab).
2. Our own page tables (drop the bootloader's) + HHDM + stacks with guard pages.
3. VMA + page fault + anonymous memory; then section/COW.
4. Page cache (together with VFS).
