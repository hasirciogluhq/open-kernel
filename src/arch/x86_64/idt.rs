//! IDT and exception/IRQ entry. Uses the compiler's `x86-interrupt` ABI, so no
//! hand-written entry stubs are needed.

use super::{cpu, gdt, pic};
use crate::kprintln;
use core::arch::asm;
use core::cell::UnsafeCell;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptStackFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Entry {
    lo: u16,
    sel: u16,
    ist: u8,
    attr: u8,
    mid: u16,
    hi: u32,
    _z: u32,
}

const EMPTY: Entry = Entry { lo: 0, sel: 0, ist: 0, attr: 0, mid: 0, hi: 0, _z: 0 };

struct Idt(UnsafeCell<[Entry; 256]>);
// SAFETY: filled during early boot before interrupts are enabled.
unsafe impl Sync for Idt {}
static IDT: Idt = Idt(UnsafeCell::new([EMPTY; 256]));

#[repr(C, packed)]
struct DescPtr {
    limit: u16,
    base: u64,
}

fn set(vec: usize, handler: usize, ist: u8) {
    let h = handler as u64;
    // SAFETY: early boot only.
    unsafe {
        (*IDT.0.get())[vec] = Entry {
            lo: h as u16,
            sel: gdt::KCODE,
            ist,
            attr: 0x8E, // present, DPL0, interrupt gate
            mid: (h >> 16) as u16,
            hi: (h >> 32) as u32,
            _z: 0,
        };
    }
}

fn dump(name: &str, f: &InterruptStackFrame, err: Option<u64>) -> ! {
    kprintln!("\n*** EXCEPTION: {} ***", name);
    if let Some(e) = err {
        kprintln!("error code {:#x}", e);
    }
    kprintln!("rip {:#x} cs {:#x} rflags {:#x} rsp {:#x}", f.rip, f.cs, f.rflags, f.rsp);
    super::backtrace::print_from(cpu::read_rbp());
    cpu::halt_forever()
}

extern "x86-interrupt" fn divide(f: InterruptStackFrame) {
    dump("#DE divide error", &f, None)
}
extern "x86-interrupt" fn breakpoint(f: InterruptStackFrame) {
    kprintln!("#BP at {:#x}", f.rip);
}
extern "x86-interrupt" fn invalid_op(f: InterruptStackFrame) {
    dump("#UD invalid opcode", &f, None)
}
extern "x86-interrupt" fn double_fault(f: InterruptStackFrame, e: u64) -> ! {
    dump("#DF double fault", &f, Some(e))
}
extern "x86-interrupt" fn general_protection(f: InterruptStackFrame, e: u64) {
    dump("#GP general protection", &f, Some(e))
}
extern "x86-interrupt" fn page_fault(f: InterruptStackFrame, e: u64) {
    kprintln!("cr2 (fault address) = {:#x}", cpu::read_cr2());
    dump("#PF page fault", &f, Some(e))
}
extern "x86-interrupt" fn timer(_f: InterruptStackFrame) {
    pic::eoi(0);
    crate::ke::sched::tick();
}
extern "x86-interrupt" fn spurious(_f: InterruptStackFrame) {}

pub fn init() {
    set(0, divide as *const () as usize, 0);
    set(3, breakpoint as *const () as usize, 0);
    set(6, invalid_op as *const () as usize, 0);
    set(8, double_fault as *const () as usize, gdt::IST_DF);
    set(13, general_protection as *const () as usize, 0);
    set(14, page_fault as *const () as usize, 0);
    set(pic::VECTOR_BASE as usize, timer as *const () as usize, 0);
    set(pic::VECTOR_BASE as usize + 7, spurious as *const () as usize, 0);
    set(pic::VECTOR_BASE as usize + 15, spurious as *const () as usize, 0);
    let dp = DescPtr { limit: (256 * 16 - 1) as u16, base: IDT.0.get() as u64 };
    // SAFETY: table is fully initialised and 'static.
    unsafe { asm!("lidt [{}]", in(reg) &dp, options(readonly, nostack, preserves_flags)) };
}
