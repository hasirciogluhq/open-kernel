# Drivers, I/O, File System, Networking

Everything is kept **simple and shallow**: first the smallest working slice, then expansion.

## 1. Driver Model

- **Device node tree** (a mix of Windows PnP and the Linux devtree): the bus driver enumerates child devices (PCI ECAM, virtio, platform/DT/ACPI) → the matching driver is `probe()`d.
- **Driver = ELF module** (ET_REL/ET_DYN) using the `include/odk/` API. Statically linked at first (together with the kernel), later a module loadable from the initrd.
- Driver contract (C ABI; Rust and C++ use the same header):

```c
struct odk_driver {
    const char *name;
    const struct odk_match *match;      // vendor/device, compatible, class
    int  (*probe)(struct odk_device *);
    void (*remove)(struct odk_device *);
    void (*handle_request)(struct odk_device *, struct odk_request *);  // IoRequest
    void (*isr)(struct odk_device *, u32 irq);                          // short; the real work goes to a DPC
};
```

- **IoRequest** (IRP-like): `{ op, buffer(s), offset, len, completion }`; it flows through the driver stack (filter → function → bus) and lands in the completion ring when done.
- **Power/PnP:** only `probe/remove` at first; suspend/resume later.
- **Driver language:** Rust by default; C/C++ also work. Kernel internal headers are not visible → drivers can be built separately from the kernel.
- **Stability:** ODK is versioned (`ODK_VERSION`); on a breaking change the module is not loaded.

### Device priority order (for QEMU/Fusion)

1. Serial (PL011, 16550) + GOP framebuffer
2. PCI/ECAM + virtio-pci (blk, net, input, gpu)
3. Storage: virtio-blk → NVMe (Fusion provides NVMe) → AHCI
4. Input: virtio-input, PS/2 (x86), USB HID (later)
5. Network: virtio-net → e1000e (Fusion)
6. Fusion-specific: VMware SVGA3, vmxnet3 (optional)

## 2. I/O Manager

- A single asynchronous core: `IoSubmit` (read from the SQ) → the relevant device stack → completion goes to the CQ.
- Blocking `read/write` are wrapped at library level with the ring + `wait`.
- Block layer: a `BlockDevice` interface (sector read/write, flush), a simple I/O scheduler (FIFO at first), integrated with the page cache.

## 3. File System

```
user: open/read/write/mmap (handle-based, over the ring)
        │
      VFS  (mount table, path resolution, dentry+inode cache, file object)
        │
   FS driver (module)  ──  Block layer  ──  disk driver
        │
   page cache (unified with mm)
```

- **VFS interface (Rust trait → C ABI):** `lookup, create, unlink, read_page, write_page, readdir, stat, truncate`.
- **Order:** `tarfs` (initrd, read-only) → `fat32` (read/write the ESP) → `ext2/ext4-ro` → **native FS** (log-structured/CoW, checksummed; much later).
- Namespace: a Windows-like object path (`\Device\...`, `\Fs\...`) + a Linux-like single-root mount tree → the user sees a **single root `/`**, with devices as objects under `/dev`.
- Security: file access is handle rights + a file ACL (simple, UNIX-mode-like at first).

## 4. Networking

```
sockets (handle-based object)  ─  TCP/UDP/ICMP  ─  IP(v4, v6 later)  ─  ARP/ND  ─  netdev
                                                                          │
                                                         NIC drivers (virtio-net, e1000e)
```

- **NetBuf:** a zero-copy-friendly packet buffer (headroom/tailroom, scatter-gather).
- **netdev interface (ODK):** `tx(NetBuf)`, `rx_poll`, `mac`, `mtu`, `link_state`; NIC interrupt → DPC → NAPI-like poll.
- **Stack:** Rust. The first step is a socket object on top of a mature `no_std` TCP/IP crate (smoltcp); our own stack only if the need arises.
- Services: the DHCP client and DNS resolver are userspace services (under `svcmgr`). The kernel provides only transport + sockets.
- Goal: `virtio-net` + DHCP + `ping` + TCP echo (milestone M7).

## 5. Rules at the Kernel ↔ Driver Boundary

- A driver uses only ODK calls for locks/memory; it cannot block or allocate in an ISR (pre-reserved pool).
- DMA buffers only through `odk_dma_*`.
- If a driver crashes (panic), the whole system panics at first; later a user-mode driver framework (UMDF-like) and driver isolation.
