//! PCI/PCIe enumeration. ECAM (from ACPI MCFG) when available, else legacy 0xCF8/0xCFC.

use super::acpi::Info as AcpiInfo;
use crate::arch::io::{inl, outl};
use crate::kprintln;
use alloc::vec::Vec;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Device {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header: u8,
}

#[derive(Clone, Copy)]
enum Access {
    Ecam { base: u64, start_bus: u8 },
    Legacy,
}

fn read32(a: Access, bus: u8, dev: u8, func: u8, off: u16) -> u32 {
    match a {
        Access::Ecam { base, start_bus } => {
            let addr = base + (((bus - start_bus) as u64) << 20 | (dev as u64) << 15 | (func as u64) << 12 | off as u64);
            // SAFETY: ECAM window from MCFG; MMIO read.
            unsafe { core::ptr::read_volatile(addr as *const u32) }
        }
        Access::Legacy => {
            let v = 0x8000_0000u32 | (bus as u32) << 16 | (dev as u32) << 11 | (func as u32) << 8 | (off as u32 & 0xFC);
            // SAFETY: PCI configuration mechanism #1.
            unsafe {
                outl(0xCF8, v);
                inl(0xCFC)
            }
        }
    }
}

pub fn enumerate(acpi: Option<&AcpiInfo>) -> Vec<Device> {
    let (access, last_bus, name) = match acpi.and_then(|a| a.mcfg.first()) {
        Some(&(base, _, s, e)) => (Access::Ecam { base, start_bus: s }, e, "ECAM"),
        None => (Access::Legacy, 255, "legacy 0xCF8"),
    };
    let first_bus = if let Access::Ecam { start_bus, .. } = access { start_bus } else { 0 };
    kprintln!("pci: enumerating via {}", name);

    let mut out = Vec::new();
    for bus in first_bus..=last_bus {
        for dev in 0..32u8 {
            if read32(access, bus, dev, 0, 0) & 0xFFFF == 0xFFFF {
                continue;
            }
            let multi = read32(access, bus, dev, 0, 0x0C) >> 16 & 0x80 != 0;
            for func in 0..(if multi { 8 } else { 1 }) {
                let id = read32(access, bus, dev, func, 0);
                if id & 0xFFFF == 0xFFFF {
                    continue;
                }
                let cls = read32(access, bus, dev, func, 8);
                out.push(Device {
                    bus,
                    dev,
                    func,
                    vendor: id as u16,
                    device: (id >> 16) as u16,
                    class: (cls >> 24) as u8,
                    subclass: (cls >> 16) as u8,
                    prog_if: (cls >> 8) as u8,
                    header: (read32(access, bus, dev, func, 0x0C) >> 16) as u8 & 0x7F,
                });
            }
        }
    }
    for d in &out {
        kprintln!(
            "pci: {:02x}:{:02x}.{} {:04x}:{:04x} {} [{}]",
            d.bus, d.dev, d.func, d.vendor, d.device, class_name(d.class, d.subclass), vendor_name(d.vendor)
        );
    }
    kprintln!("pci: {} function(s)", out.len());
    out
}

pub fn class_name(c: u8, s: u8) -> &'static str {
    match (c, s) {
        (0x00, _) => "unclassified",
        (0x01, 0x01) => "IDE controller",
        (0x01, 0x06) => "SATA controller",
        (0x01, 0x08) => "NVMe controller",
        (0x01, _) => "storage controller",
        (0x02, 0x00) => "ethernet controller",
        (0x02, _) => "network controller",
        (0x03, _) => "display controller",
        (0x04, _) => "multimedia",
        (0x05, _) => "memory controller",
        (0x06, 0x00) => "host bridge",
        (0x06, 0x01) => "ISA bridge",
        (0x06, 0x04) => "PCI-PCI bridge",
        (0x06, _) => "bridge",
        (0x07, _) => "communication controller",
        (0x08, _) => "system peripheral",
        (0x09, _) => "input device",
        (0x0C, 0x03) => "USB controller",
        (0x0C, 0x05) => "SMBus",
        (0x0C, _) => "serial bus",
        _ => "other",
    }
}

pub fn vendor_name(v: u16) -> &'static str {
    match v {
        0x8086 => "Intel",
        0x1022 => "AMD",
        0x1002 => "AMD/ATI",
        0x10DE => "NVIDIA",
        0x1AF4 => "Red Hat virtio",
        0x1B36 => "Red Hat/QEMU",
        0x1234 => "QEMU/Bochs",
        0x15AD => "VMware",
        0x10EC => "Realtek",
        _ => "?",
    }
}
