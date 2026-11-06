//! SMBIOS: BIOS, system and baseboard ("motherboard") identification.

use super::{cstr, rd};
use crate::kprintln;

/// Nth string (1-based) of the structure at `s` whose formatted area is `len` bytes.
fn string(s: usize, len: usize, idx: u8) -> &'static str {
    if idx == 0 {
        return "";
    }
    let mut p = s + len;
    for _ in 1..idx {
        // SAFETY: walks the NUL-separated string set of a firmware table.
        while unsafe { rd::<u8>(p) } != 0 {
            p += 1;
        }
        p += 1;
    }
    cstr(p, 128)
}

pub fn init(entry: u64, v3: bool) {
    if entry == 0 {
        kprintln!("smbios: not provided by firmware");
        return;
    }
    let e = entry as usize;
    // SAFETY: entry point structure from the firmware configuration table.
    let (table, tlen) = unsafe {
        if v3 {
            (rd::<u64>(e + 0x10) as usize, rd::<u32>(e + 0x0C) as usize)
        } else {
            (rd::<u32>(e + 0x18) as usize, rd::<u16>(e + 0x16) as usize)
        }
    };
    let (mut major, mut minor) = unsafe { (rd::<u8>(e + 6), rd::<u8>(e + 7)) };
    if v3 {
        (major, minor) = unsafe { (rd::<u8>(e + 7), rd::<u8>(e + 8)) };
    }
    kprintln!("smbios: v{}.{} ({}), table {:#x} len {}", major, minor, if v3 { "64-bit" } else { "32-bit" }, table, tlen);

    let mut p = table;
    let end = table + tlen;
    while p + 4 <= end {
        let (ty, len) = unsafe { (rd::<u8>(p), rd::<u8>(p + 1) as usize) };
        if ty == 127 || len < 4 {
            break;
        }
        let b = |o: usize| -> u8 { if o < len { unsafe { rd::<u8>(p + o) } } else { 0 } };
        match ty {
            0 => kprintln!("smbios: BIOS      {} {}", string(p, len, b(4)), string(p, len, b(5))),
            1 => kprintln!("smbios: system    {} / {} (serial '{}')", string(p, len, b(4)), string(p, len, b(5)), string(p, len, b(7))),
            2 => kprintln!(
                "smbios: baseboard {} / {} v'{}' (serial '{}')",
                string(p, len, b(4)),
                string(p, len, b(5)),
                string(p, len, b(6)),
                string(p, len, b(7))
            ),
            4 => kprintln!("smbios: processor {}", string(p, len, b(0x10))),
            _ => {}
        }
        // Skip formatted area, then the string set (terminated by a double NUL).
        let mut q = p + len;
        loop {
            // SAFETY: bounded by table end below.
            if unsafe { rd::<u16>(q) } == 0 {
                q += 2;
                break;
            }
            q += 1;
            if q + 2 > end {
                break;
            }
        }
        p = q;
    }
}
