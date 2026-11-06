//! Hardware abstraction: console, firmware tables, bus enumeration.

#[path = "acpi/acpi.rs"]
pub mod acpi;
#[path = "pci/pci.rs"]
pub mod pci;
#[path = "console/serial.rs"]
pub mod serial;
#[path = "smbios/smbios.rs"]
pub mod smbios;

/// Read helper for firmware tables (identity-mapped physical memory, unaligned).
///
/// # Safety
/// `addr` must be readable memory.
pub unsafe fn rd<T: Copy>(addr: usize) -> T {
    unsafe { core::ptr::read_unaligned(addr as *const T) }
}

pub fn cstr<'a>(addr: usize, max: usize) -> &'a str {
    let mut n = 0;
    // SAFETY: bounded scan of firmware strings.
    unsafe {
        while n < max && *((addr + n) as *const u8) != 0 {
            n += 1;
        }
        core::str::from_utf8(core::slice::from_raw_parts(addr as *const u8, n)).unwrap_or("?")
    }
}
