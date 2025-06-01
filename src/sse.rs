use core::arch::asm;

/// Enable SSE instructions by configuring control registers.
///
/// # Safety
/// This function performs privileged register modifications and must
/// only be called during early kernel initialization before any
/// floating point operations are executed.
pub unsafe fn init() {
    let mut cr0: u64;
    asm!("mov {}, cr0", out(reg) cr0, options(nostack, preserves_flags));
    // clear EM (bit 2) and TS (bit 3)
    cr0 &= !((1 << 2) | (1 << 3));
    // set MP (bit 1) and NE (bit 5)
    cr0 |= (1 << 1) | (1 << 5);
    asm!("mov cr0, {}", in(reg) cr0, options(nostack, preserves_flags));

    let mut cr4: u64;
    asm!("mov {}, cr4", out(reg) cr4, options(nostack, preserves_flags));
    // set OSFXSR (bit 9) and OSXMMEXCPT (bit 10)
    cr4 |= (1 << 9) | (1 << 10);
    asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
}
