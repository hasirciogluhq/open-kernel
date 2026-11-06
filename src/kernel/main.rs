#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

// Layout follows plan/folder-structure.md: arch/ and hal/ live beside kernel/.
#[path = "../arch/x86_64/mod.rs"]
mod arch;
mod bootinfo;
#[path = "../hal/mod.rs"]
mod hal;
mod ke;
mod mm;

use arch::cpu;
use bootinfo::{BootInfo, FB_BGRX, FB_NONE, FB_RGBX};
use core::panic::PanicInfo;
use ke::sched;

/// Kernel entry, called by `opl-boot` with `rdi = &OplBootInfo` (SysV ABI).
/// The boot context keeps running on the firmware stack as thread "boot".
#[unsafe(no_mangle)]
pub extern "sysv64" fn _start(boot_info: *const BootInfo) -> ! {
    hal::serial::init();
    kprintln!("\nOPL kernel {} (x86_64)", env!("CARGO_PKG_VERSION"));

    // SAFETY: the bootloader passes a pointer to its BootInfo.
    let Some(bi) = (unsafe { BootInfo::from_ptr(boot_info) }) else {
        kprintln!("bootinfo: invalid magic/version/size");
        cpu::halt_forever();
    };

    // ── memory ──
    let (free_mib, heap_mib) = mm::init(bi);
    kprintln!("memory: {} MiB free frames, {} MiB kernel heap", free_mib, heap_mib);

    // ── CPU ──
    let (vendor, brand, apic) = cpu::cpu_identity();
    kprintln!(
        "cpu: {} '{}' (initial APIC id {})",
        core::str::from_utf8(&vendor).unwrap_or("?"),
        core::str::from_utf8(&brand).unwrap_or("?").trim_matches(|c: char| c == '\0' || c == ' '),
        apic
    );
    arch::gdt::init();
    arch::idt::init();
    kprintln!("cpu: GDT/TSS/IDT loaded (IST1 for #DF)");

    // ── platform: firmware tables, board, buses, UARTs ──
    let acpi = hal::acpi::init(bi.acpi_rsdp);
    hal::smbios::init(bi.smbios, bi.smbios_v3 != 0);
    let _pci = hal::pci::enumerate(acpi.as_ref());
    let ports = hal::serial::probe_all();
    for (i, p) in ports.iter().enumerate() {
        if let Some(p) = p {
            kprintln!("serial: COM{} present at {:#x}", i + 1, p);
        }
    }

    if bi.fb_format != FB_NONE {
        kprintln!("fb: {}x{} pitch {} @ {:#x}", bi.fb_width, bi.fb_height, bi.fb_pitch, bi.fb_base);
        paint_banner(bi);
    }

    // ── threads ──
    sched::init();
    arch::pic::init();
    cpu::enable_interrupts();
    kprintln!("sched: timer at {} Hz, preemptive round-robin, slice 10 ms", arch::pic::HZ);
    thread_demo();

    kprintln!("boot ok, idling");
    loop {
        sched::sleep_ms(1000);
    }
}

fn worker_ticker(n: usize) {
    for i in 0..n {
        kprintln!("[ticker  t={:>4}] step {}", sched::ticks(), i);
        sched::sleep_ms(100);
    }
}

fn worker_spin(id: usize) {
    // Never sleeps or yields: proves the timer preempts busy threads.
    let mut rounds = 0u32;
    let mut next = sched::ticks() + 50;
    while rounds < 3 {
        if sched::ticks() >= next {
            kprintln!("[spin{}   t={:>4}] still running (preempted fairly)", id, sched::ticks());
            next += 50;
            rounds += 1;
        }
    }
}

fn worker_pausable(_: usize) {
    for i in 0..4 {
        kprintln!("[pausable t={:>4}] work {}", sched::ticks(), i);
        sched::sleep_ms(40);
    }
}

fn worker_forever(_: usize) {
    loop {
        sched::sleep_ms(30);
    }
}

fn thread_demo() {
    let ticker = sched::spawn("ticker", |_| worker_ticker(4), 0);
    let s1 = sched::spawn("spin1", worker_spin, 1);
    let s2 = sched::spawn("spin2", worker_spin, 2);
    let pausable = sched::spawn("pausable", worker_pausable, 0);
    let forever = sched::spawn("forever", worker_forever, 0);

    sched::sleep_ms(50);
    kprintln!("[boot    t={:>4}] suspend pausable", sched::ticks());
    sched::suspend(pausable);
    sched::sleep_ms(150);
    kprintln!("[boot    t={:>4}] resume pausable", sched::ticks());
    sched::resume(pausable);
    sched::dump();
    kprintln!("[boot    t={:>4}] terminate forever", sched::ticks());
    sched::terminate(forever);

    for t in [ticker, s1, s2, pausable] {
        sched::join(t);
    }
    sched::sleep_ms(10); // let the scheduler reap
    kprintln!("[boot    t={:>4}] all workers joined", sched::ticks());
    sched::dump();
}

/// Fill the top 32 rows with a solid colour to prove the framebuffer works.
fn paint_banner(bi: &BootInfo) {
    let (r, g, b) = (0x20u8, 0x90u8, 0xE0u8);
    let px: [u8; 4] = match bi.fb_format {
        FB_RGBX => [r, g, b, 0],
        FB_BGRX => [b, g, r, 0],
        _ => return,
    };
    let rows = bi.fb_height.min(32) as usize;
    for y in 0..rows {
        for x in 0..bi.fb_width as usize {
            let off = y * bi.fb_pitch as usize + x * 4;
            // SAFETY: within the GOP framebuffer reported by firmware.
            unsafe { core::ptr::write_volatile((bi.fb_base as usize + off) as *mut [u8; 4], px) };
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cpu::disable_interrupts();
    kprintln!("\n*** KERNEL PANIC ***\n{}", info);
    arch::backtrace::print_from(cpu::read_rbp());
    cpu::halt_forever()
}
