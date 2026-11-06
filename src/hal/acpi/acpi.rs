//! ACPI table discovery: RSDP -> XSDT/RSDT -> FADT/MADT/MCFG (+ list of all tables).

use super::{cstr, rd};
use crate::kprintln;
use alloc::vec::Vec;

#[allow(dead_code)]
pub struct Info {
    pub oem: [u8; 6],
    pub cpus: u32,
    pub lapic_addr: u32,
    pub ioapics: u32,
    /// (ECAM base, segment, start bus, end bus)
    pub mcfg: Vec<(u64, u16, u8, u8)>,
}

fn sig(addr: usize) -> [u8; 4] {
    unsafe { rd::<[u8; 4]>(addr) }
}

fn checksum_ok(addr: usize, len: usize) -> bool {
    let mut s = 0u8;
    for i in 0..len {
        s = s.wrapping_add(unsafe { rd::<u8>(addr + i) });
    }
    s == 0
}

pub fn init(rsdp: u64) -> Option<Info> {
    if rsdp == 0 {
        kprintln!("acpi: no RSDP");
        return None;
    }
    let r = rsdp as usize;
    // SAFETY: firmware-provided, identity mapped.
    let (rev, rsdt, xsdt) = unsafe { (rd::<u8>(r + 15), rd::<u32>(r + 16) as u64, rd::<u64>(r + 24)) };
    if unsafe { rd::<[u8; 8]>(r) } != *b"RSD PTR " || !checksum_ok(r, 20) {
        kprintln!("acpi: bad RSDP");
        return None;
    }
    let oem = unsafe { rd::<[u8; 6]>(r + 9) };
    kprintln!("acpi: RSDP rev {} oem '{}'", rev, cstr_n(&oem));

    let (root, wide) = if rev >= 2 && xsdt != 0 { (xsdt as usize, true) } else { (rsdt as usize, false) };
    let len = unsafe { rd::<u32>(root + 4) } as usize;
    let esz = if wide { 8 } else { 4 };
    let n = len.saturating_sub(36) / esz;

    let mut info = Info { oem, cpus: 0, lapic_addr: 0, ioapics: 0, mcfg: Vec::new() };
    let mut names = alloc::string::String::new();
    for i in 0..n {
        let t = if wide { unsafe { rd::<u64>(root + 36 + i * 8) as usize } } else { unsafe { rd::<u32>(root + 36 + i * 4) as usize } };
        let s = sig(t);
        names.push_str(core::str::from_utf8(&s).unwrap_or("????"));
        names.push(' ');
        let tlen = unsafe { rd::<u32>(t + 4) } as usize;
        match &s {
            b"APIC" => parse_madt(t, tlen, &mut info),
            b"MCFG" => {
                let mut o = 44;
                while o + 16 <= tlen {
                    unsafe {
                        info.mcfg.push((rd::<u64>(t + o), rd::<u16>(t + o + 8), rd::<u8>(t + o + 10), rd::<u8>(t + o + 11)));
                    }
                    o += 16;
                }
            }
            _ => {}
        }
    }
    kprintln!("acpi: {} tables: {}", n, names);
    kprintln!("acpi: {} CPU(s) in MADT, {} IOAPIC(s), LAPIC @ {:#x}", info.cpus, info.ioapics, info.lapic_addr);
    for m in &info.mcfg {
        kprintln!("acpi: MCFG ECAM {:#x} seg {} buses {}-{}", m.0, m.1, m.2, m.3);
    }
    Some(info)
}

fn parse_madt(t: usize, len: usize, info: &mut Info) {
    info.lapic_addr = unsafe { rd::<u32>(t + 36) };
    let mut o = 44;
    while o + 2 <= len {
        let (ty, l) = unsafe { (rd::<u8>(t + o), rd::<u8>(t + o + 1) as usize) };
        if l < 2 {
            break;
        }
        match ty {
            0 => {
                // Processor Local APIC: flags bit0 enabled, bit1 online-capable
                if unsafe { rd::<u32>(t + o + 4) } & 3 != 0 {
                    info.cpus += 1;
                }
            }
            1 => info.ioapics += 1,
            _ => {}
        }
        o += l;
    }
}

fn cstr_n(b: &[u8]) -> &str {
    core::str::from_utf8(b).unwrap_or("?").trim()
}

#[allow(dead_code)]
pub fn name_at(addr: usize) -> &'static str {
    cstr(addr, 8)
}
