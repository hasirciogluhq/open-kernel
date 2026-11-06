//! Context switch: the one piece of unavoidable assembly in the scheduler.

use core::arch::naked_asm;

/// Save callee-saved registers on the current stack, store the stack pointer in
/// `*old_sp`, load `new_sp` and restore the registers pushed there.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn switch_context(old_sp: *mut usize, new_sp: usize) {
    naked_asm!(
        "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp",
        "ret",
    )
}

/// Prepare a fresh stack so the first `switch_context` into it "returns" to `entry`.
/// Returns the initial stack pointer.
pub fn init_stack(top: usize, entry: extern "sysv64" fn() -> !) -> usize {
    let top = top & !0xF;
    // SAFETY: caller owns [top - 64, top).
    unsafe {
        let p = top as *mut usize;
        *p.sub(1) = 0; // fake return address: entry sees rsp % 16 == 8
        *p.sub(2) = entry as usize; // popped by `ret`
        for i in 3..=8 {
            *p.sub(i) = 0; // rbp rbx r12 r13 r14 r15
        }
    }
    top - 8 * 8
}
