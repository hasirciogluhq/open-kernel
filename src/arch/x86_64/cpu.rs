//! CPU primitives: interrupt flag, halt, control registers, CPUID.

use core::arch::asm;

pub fn halt_forever() -> ! {
    disable_interrupts();
    loop {
        // SAFETY: `hlt` has no memory effects.
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[inline]
pub fn disable_interrupts() {
    unsafe { asm!("cli", options(nomem, nostack)) };
}
#[inline]
pub fn enable_interrupts() {
    unsafe { asm!("sti", options(nomem, nostack)) };
}
/// `sti; hlt` — atomic enough that no interrupt is lost between the two.
#[inline]
pub fn enable_and_hlt() {
    unsafe { asm!("sti", "hlt", options(nomem, nostack)) };
}

#[inline]
pub fn interrupts_enabled() -> bool {
    let f: u64;
    unsafe { asm!("pushfq", "pop {}", out(reg) f, options(nomem, preserves_flags)) };
    f & (1 << 9) != 0
}

pub fn without_interrupts<R>(f: impl FnOnce() -> R) -> R {
    let was = interrupts_enabled();
    disable_interrupts();
    let r = f();
    if was {
        enable_interrupts();
    }
    r
}

pub fn read_cr2() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr2", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

pub fn read_rbp() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, rbp", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// (vendor, brand, initial APIC id)
pub fn cpu_identity() -> ([u8; 12], [u8; 48], u32) {
    use core::arch::x86_64::__cpuid;
    let mut vendor = [0u8; 12];
    let mut brand = [0u8; 48];
    {
        let r = __cpuid(0);
        vendor[0..4].copy_from_slice(&r.ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&r.edx.to_le_bytes());
        vendor[8..12].copy_from_slice(&r.ecx.to_le_bytes());
        if __cpuid(0x8000_0000).eax >= 0x8000_0004 {
            for i in 0..3u32 {
                let r = __cpuid(0x8000_0002 + i);
                let o = i as usize * 16;
                brand[o..o + 4].copy_from_slice(&r.eax.to_le_bytes());
                brand[o + 4..o + 8].copy_from_slice(&r.ebx.to_le_bytes());
                brand[o + 8..o + 12].copy_from_slice(&r.ecx.to_le_bytes());
                brand[o + 12..o + 16].copy_from_slice(&r.edx.to_le_bytes());
            }
        }
        (vendor, brand, __cpuid(1).ebx >> 24)
    }
}
