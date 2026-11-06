//! Mirror of `include/boot/bootinfo.h`. Keep both in sync.

pub const MAGIC: u64 = 0x0054_4F4F_424C_504F;
pub const VERSION: u32 = 1;

pub const FB_NONE: u32 = 0;
pub const FB_RGBX: u32 = 1;
pub const FB_BGRX: u32 = 2;

#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub size: u32,
    pub mmap: u64,
    pub mmap_size: u64,
    pub mmap_desc_size: u64,
    pub mmap_desc_version: u32,
    _pad0: u32,
    pub fb_base: u64,
    pub fb_size: u64,
    pub fb_width: u32,
    pub fb_height: u32,
    pub fb_pitch: u32,
    pub fb_format: u32,
    pub acpi_rsdp: u64,
    pub smbios: u64,
    pub smbios_v3: u32,
    _pad1: u32,
}

const _: () = assert!(core::mem::size_of::<BootInfo>() == 104);

/// UEFI `EFI_MEMORY_DESCRIPTOR` (fixed prefix; real stride is `mmap_desc_size`).
#[repr(C)]
pub struct MemDesc {
    pub kind: u32,
    _pad: u32,
    pub phys: u64,
    pub virt: u64,
    pub pages: u64,
    pub attr: u64,
}

impl BootInfo {
    /// Validate the handoff; `None` if magic/version/size mismatch.
    ///
    /// # Safety
    /// `p` must point to readable memory of at least `size_of::<BootInfo>()` bytes.
    pub unsafe fn from_ptr<'a>(p: *const BootInfo) -> Option<&'a BootInfo> {
        if p.is_null() {
            return None;
        }
        // SAFETY: caller guarantees readability.
        let b = unsafe { &*p };
        (b.magic == MAGIC
            && b.version == VERSION
            && b.size as usize == core::mem::size_of::<BootInfo>())
        .then_some(b)
    }

    /// Iterate the UEFI memory map.
    pub fn memory_map(&self) -> impl Iterator<Item = &MemDesc> {
        let stride = self.mmap_desc_size as usize;
        let count = if stride == 0 { 0 } else { self.mmap_size as usize / stride };
        (0..count).map(move |i| {
            // SAFETY: bootloader guarantees `count` descriptors of `stride` bytes at `mmap`.
            unsafe { &*((self.mmap as usize + i * stride) as *const MemDesc) }
        })
    }
}
