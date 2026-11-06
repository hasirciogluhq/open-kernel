//! Legacy 8259 PIC + 8254 PIT: the first timer tick source. (LAPIC/IOAPIC come with SMP.)

use super::io::{inb, outb};

pub const VECTOR_BASE: u8 = 0x20;
pub const HZ: u32 = 1000;

pub fn init() {
    // SAFETY: standard PIC/PIT programming sequences.
    unsafe {
        outb(0x20, 0x11); outb(0xA0, 0x11);                    // ICW1: init, expect ICW4
        outb(0x21, VECTOR_BASE); outb(0xA1, VECTOR_BASE + 8);  // ICW2: vector offsets
        outb(0x21, 4); outb(0xA1, 2);                          // ICW3: cascade on IRQ2
        outb(0x21, 1); outb(0xA1, 1);                          // ICW4: 8086 mode
        outb(0x21, 0xFE); outb(0xA1, 0xFF);                    // unmask IRQ0 only

        let div = (1_193_182 / HZ) as u16;
        outb(0x43, 0x34); // ch0, lo/hi, rate generator
        outb(0x40, div as u8);
        outb(0x40, (div >> 8) as u8);
    }
}

pub fn eoi(irq: u8) {
    // SAFETY: EOI command to PIC(s).
    unsafe {
        if irq >= 8 {
            outb(0xA0, 0x20);
        }
        outb(0x20, 0x20);
    }
}

#[allow(dead_code)]
pub fn mask_state() -> (u8, u8) {
    // SAFETY: reads the PIC mask registers.
    unsafe { (inb(0x21), inb(0xA1)) }
}
