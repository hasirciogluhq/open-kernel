//! 16550 UART: COM1 early console plus detection of COM1..COM4.

use crate::arch::io::{inb, outb};
use core::fmt;

const PORTS: [u16; 4] = [0x3F8, 0x2F8, 0x3E8, 0x2E8];
const COM1: u16 = 0x3F8;

pub fn init() {
    // SAFETY: standard 16550 init on COM1: 115200 8N1, FIFO on.
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x01);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xC7);
        outb(COM1 + 4, 0x03);
    }
}

/// Probe a UART with the scratch register + loopback mode. Returns true if one exists.
fn present(base: u16) -> bool {
    // SAFETY: probing legacy UART ports; harmless if nothing is there.
    unsafe {
        outb(base + 7, 0xA5);
        if inb(base + 7) != 0xA5 {
            return false;
        }
        outb(base + 7, 0x5A);
        if inb(base + 7) != 0x5A {
            return false;
        }
        // Loopback self test.
        let old = inb(base + 4);
        outb(base + 4, 0x1E);
        outb(base, 0xAE);
        let ok = inb(base) == 0xAE;
        outb(base + 4, old);
        ok
    }
}

pub fn probe_all() -> [Option<u16>; 4] {
    let mut r = [None; 4];
    for (i, p) in PORTS.iter().enumerate() {
        if present(*p) {
            r[i] = Some(*p);
        }
    }
    r
}

pub fn put(b: u8) {
    // SAFETY: COM1 status/data registers.
    unsafe {
        let mut spins = 0u32;
        while inb(COM1 + 5) & 0x20 == 0 && spins < 1_000_000 {
            spins += 1;
        }
        outb(COM1, b);
    }
}

pub struct Serial;

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                put(b'\r');
            }
            put(b);
        }
        Ok(())
    }
}

/// Atomic w.r.t. preemption: a whole line is written with interrupts off.
#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprintln!("") };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        $crate::arch::cpu::without_interrupts(|| {
            let _ = writeln!($crate::hal::serial::Serial, $($arg)*);
        });
    }};
}
