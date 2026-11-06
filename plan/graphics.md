# Graphics (a stack separate from the kernel)

**Decision:** the graphics API, rasterizer, shader compiler and compositor are **not in the kernel** and are built separately under `gfx/`. The kernel only provides the mechanism for safe access to the hardware. This takes the layering idea from `docs/new-graphics-system-idea.md` (API → subsystem → UMD → KMD) without burdening the kernel.

## 1. Division of Responsibility

```
┌──────────────────────── User Mode ─────────────────────────┐
│ Application / game                                          │
│   ogfx API  (gfx/include/ogfx, C ABI; Vulkan-like, open)    │
│   ogfx runtime = "router/loader"                            │
│        lists devices, picks a UMD from the manifest,        │
│        manages resources / fences / swapchain               │
│   UMD (ICD) plugins:  soft | virtio | vmware | amd |        │
│                       nvidia | intel                        │
│   compositor / display server (window composition, present) │
├──────────────────── syscalls + handles ─────────────────────┤
│ Kernel (mechanism ONLY):                                    │
│   gpu device class (ODK) ── KMDs (drivers/display/*)        │
│   BufferObject (section-based, shareable, pinnable)         │
│   Fence/SyncObject (dispatcher object)                      │
│   CommandSubmit ring (passes an opaque command buffer to KMD)│
│   Modeset/Scanout (set mode, bind buffer to screen, vblank) │
│   MMIO/DMA/IRQ isolation, per-process GPU address space     │
└─────────────────────────────────────────────────────────────┘
```

- The kernel does **not interpret the contents** of a command buffer (at most basic validation/bounds checks); its meaning is known to the UMD and the hardware.
- "We are not writing NVIDIA/AMD drivers": the runtime is a **router**; if a UMD+KMD pair exists it picks it, otherwise it falls back to `soft`. Real GPU drivers are a long-term, separate effort.

## 2. Kernel-side Interface (thin, `include/uapi/gpu.h`)

| Syscall / object | Meaning |
|---|---|
| `GpuEnum`, `GpuOpen(dev)` | handle to a GPU/display device |
| `GpuBoCreate/Map/Export/Import` | buffer object (VRAM, system RAM, shared) |
| `GpuSubmit(ctx, cmdbuf, fences_in, fence_out)` | submit commands (opaque) |
| `GpuFenceWait/Signal` | synchronization (`wait_many` together with other dispatcher objects) |
| `DispModeList/Set`, `DispScanout(bo)`, `DispFlip`, `DispVblankEvent` | KMS-like display management |

This interface **alone** is enough for the `soft` UMD: the CPU rasterizer draws into a BufferObject and presents it with `DispFlip`.

## 3. Backend Order (focused on our environment)

| # | Backend | Where it runs | What it provides |
|---|---|---|---|
| 0 | **gop-fb KMD** | everywhere (framebuffer from the bootloader) | scanout, no mode switching |
| 1 | **soft UMD** (CPU rasterizer) | everywhere | full software 3D+2D, fallback when there is no GPU |
| 2 | **virtio-gpu 2D** | QEMU | mode setting, resource flush |
| 3 | **virtio-gpu 3D** (virgl/venus) | QEMU (limited on macOS: via ANGLE/Metal) | host GPU acceleration |
| 4 | **VMware SVGA3** | **Fusion** (ARM64 and Intel) | Fusion 3D acceleration, VMware command protocol |
| 5 | amd / nvidia / intel | real hardware | long-term only, a separate project |

An honest warning: writing a driver for the real Apple GPU (AGX) on Apple Silicon is not practical; on a macOS host, 3D acceleration comes through a **virtual GPU** (virtio/VMware). So the 3D path = "3 (QEMU virtio-gpu)" + "4 (Fusion SVGA3)".

## 4. CPU Rasterizer (soft UMD) — "both 3D and CPU raster"

- New generation, tile-based, multi-threaded (each tile is a job; the `ke` thread pool or a user thread pool).
- Pipeline: vertex processing → clip → setup → tile bin → raster (half-space/edge functions) → fragment → depth/stencil/blend → render target.
- SIMD: aarch64 NEON, x86_64 SSE4.2/AVX2; selected at runtime.
- Shaders: at first fixed/lambda "shader" functions written in C++; later a SPIR-V interpreter → JIT or ahead-of-time compilation (`gfx/shaderc`).
- Languages: C++26 (core loops), Rust (SPIR-V parsing/IR, validation), C (API headers).
- Output: a Section-based BufferObject; the compositor shares the same buffer (zero-copy).

## 5. API Design (ogfx)

- Open, low-level, command-buffer based (in the spirit of Vulkan/D3D12): `Device, Queue, CommandBuffer, Buffer, Image, Pipeline, Fence, Swapchain`.
- C ABI header + C++ and Rust wrappers.
- Later on top: a simple 2D (canvas) API, an OpenGL ES / Vulkan compatibility layer (optional, not a goal).
- The runtime loads UMDs as `gfx/umd/<name>/manifest` + a shared library (ICD model); the PCI vendor/device → UMD mapping is in the manifest.

## 6. Display Server / Compositor

- A separate user process (`gfx/display`). Window surfaces = BufferObjects; the compositor composes them and presents with `DispFlip`.
- Input events from the `input` drivers are dispatched through the compositor.
- Start: a single full-screen surface → then window management. Software or GPU compositing, same API.

## 7. Milestones (graphics)

1. **G0:** `gop-fb` KMD + the kernel's early console (not a graphics API, just logging).
2. **G1:** BufferObject + Fence + `DispScanout/Flip` syscalls; a triangle on screen with `gfx/soft`.
3. **G2:** Rasterizer: depth, texture, blend, multi-threading; a simple demo (spinning cube).
4. **G3:** virtio-gpu 2D → 3D.
5. **G4:** VMware SVGA3 (Fusion).
6. **G5:** Compositor, windows, input.

For G0–G1 the kernel tree only gains a small KMD + syscalls; everything else lives in `gfx/`.
