//! APIC subsystem: detection, ACPI MADT parsing, LAPIC + I/O APIC init.
//!
//! On success the legacy 8259 PIC is masked and all interrupt routing
//! goes through the I/O APIC → LAPIC path.  On failure the kernel
//! keeps using the PIC (which is always initialised as a fallback).

use crate::log;
use spin::Mutex;

pub mod ioapic;
pub mod lapic;

/// Maximum number of I/O APICs we track.
const MAX_IOAPICS: usize = 4;

/// Maximum number of interrupt source overrides.
const MAX_OVERRIDES: usize = 16;

/// A MADT interrupt source override entry.
#[derive(Debug, Clone, Copy)]
pub struct IrqOverride {
    pub source: u8,
    pub gsi: u32,
    #[allow(dead_code)]
    pub flags: u16,
}

/// Parsed result of MADT walk.
#[derive(Debug)]
pub struct ApicInfo {
    pub lapic_phys_base: u64,
    pub ioapic_count: usize,
    pub ioapic_addrs: [u64; MAX_IOAPICS],
    pub ioapic_gsi_bases: [u32; MAX_IOAPICS],
    pub override_count: usize,
    pub overrides: [IrqOverride; MAX_OVERRIDES],
}

/// Runtime state — only set when APIC initialisation succeeds.
struct ApicState {
    _lapic: lapic::Lapic,
}

unsafe impl Send for ApicState {}
unsafe impl Sync for ApicState {}

/// Global APIC state.
static APIC: Mutex<Option<ApicState>> = Mutex::new(None);

/// Fast-path EOI pointer.  Written once during init, then read in every
/// ISR.  `null_mut()` means APIC is not active → caller must fall back
/// to PIC EOI.
static mut LAPIC_EOI_PTR: *mut u32 = core::ptr::null_mut();

// ---------------------------------------------------------------------------
// ACPI helpers
// ---------------------------------------------------------------------------

/// Standard ACPI table header (36 bytes).
#[repr(C, packed)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    _revision: u8,
    _checksum: u8,
    _oemid: [u8; 6],
    _oem_table_id: [u8; 8],
    _oem_revision: u32,
    _creator_id: u32,
    _creator_revision: u32,
}

/// RSDP structure — we only need the bare minimum fields.
#[repr(C, packed)]
struct RsdpV1 {
    signature: [u8; 8],
    _checksum: u8,
    _oemid: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
struct RsdpV2 {
    v1: RsdpV1,
    length: u32,
    xsdt_address: u64,
    _ext_checksum: u8,
    _reserved: [u8; 3],
}

unsafe fn phys_to_virt(phys: u64, offset: u64) -> *const u8 {
    (offset + phys) as *const u8
}

pub fn check_signature(ptr: *const u8, expected: &[u8; 4]) -> bool {
    if ptr.is_null() {
        return false;
    }
    unsafe { &*(ptr as *const [u8; 4]) == expected }
}

unsafe fn find_madt_rsdt(rsdt_ptr: *const u8, phys_offset: u64) -> Option<*const u8> {
    let header = unsafe { &*(rsdt_ptr as *const SdtHeader) };
    if !check_signature(rsdt_ptr, b"RSDT") {
        return None;
    }
    let entry_count = (header.length as usize - core::mem::size_of::<SdtHeader>()) / 4;
    let entries = unsafe { rsdt_ptr.add(core::mem::size_of::<SdtHeader>()) } as *const u32;

    for i in 0..entry_count {
        let entry_phys = unsafe { *entries.add(i) } as u64;
        let entry_ptr = unsafe { phys_to_virt(entry_phys, phys_offset) };
        if check_signature(entry_ptr, b"APIC") {
            return Some(entry_ptr);
        }
    }
    None
}

unsafe fn find_madt_xsdt(xsdt_ptr: *const u8, phys_offset: u64) -> Option<*const u8> {
    let header = unsafe { &*(xsdt_ptr as *const SdtHeader) };
    if !check_signature(xsdt_ptr, b"XSDT") {
        return None;
    }
    let entry_count = (header.length as usize - core::mem::size_of::<SdtHeader>()) / 8;
    let entries = unsafe { xsdt_ptr.add(core::mem::size_of::<SdtHeader>()) } as *const u64;

    for i in 0..entry_count {
        let entry_phys = unsafe { *entries.add(i) };
        let entry_ptr = unsafe { phys_to_virt(entry_phys, phys_offset) };
        if check_signature(entry_ptr, b"APIC") {
            return Some(entry_ptr);
        }
    }
    None
}

unsafe fn parse_rsdp(rsdp_phys: u64, phys_offset: u64) -> Option<*const u8> {
    let ptr = unsafe { phys_to_virt(rsdp_phys, phys_offset) };
    let v1 = unsafe { &*(ptr as *const RsdpV1) };

    let sig: &[u8] = unsafe { core::slice::from_raw_parts(ptr, 8) };
    if sig != b"RSD PTR " {
        return None;
    }

    if v1.revision >= 2 {
        let v2 = unsafe { &*(ptr as *const RsdpV2) };
        if v2.xsdt_address != 0 {
            let xsdt = unsafe { phys_to_virt(v2.xsdt_address, phys_offset) };
            return unsafe { find_madt_xsdt(xsdt, phys_offset) };
        }
    }

    let rsdt = unsafe { phys_to_virt(v1.rsdt_address as u64, phys_offset) };
    unsafe { find_madt_rsdt(rsdt, phys_offset) }
}

unsafe fn parse_madt(madt_ptr: *const u8) -> ApicInfo {
    let header = unsafe { &*(madt_ptr as *const SdtHeader) };
    let table_end = madt_ptr as usize + header.length as usize;

    let lapic_phys_base = unsafe { *(madt_ptr.add(36) as *const u32) } as u64;

    let mut info = ApicInfo {
        lapic_phys_base,
        ioapic_count: 0,
        ioapic_addrs: [0u64; MAX_IOAPICS],
        ioapic_gsi_bases: [0u32; MAX_IOAPICS],
        override_count: 0,
        overrides: [IrqOverride {
            source: 0,
            gsi: 0,
            flags: 0,
        }; MAX_OVERRIDES],
    };

    let mut offset = 44usize;

    while offset + 2 <= (table_end - madt_ptr as usize) {
        let entry_type = unsafe { *madt_ptr.add(offset) };
        let entry_len = unsafe { *madt_ptr.add(offset + 1) } as usize;

        if entry_len < 2 {
            break;
        }
        if offset + entry_len > (table_end - madt_ptr as usize) {
            break;
        }

        match entry_type {
            1 if info.ioapic_count < MAX_IOAPICS => {
                let ioapic_id = unsafe { *madt_ptr.add(offset + 2) };
                let ioapic_addr = unsafe { *(madt_ptr.add(offset + 4) as *const u32) } as u64;
                let gsi_base = unsafe { *(madt_ptr.add(offset + 8) as *const u32) };
                info.ioapic_addrs[info.ioapic_count] = ioapic_addr;
                info.ioapic_gsi_bases[info.ioapic_count] = gsi_base;
                info.ioapic_count += 1;

                log::trace!(
                    "IOAPIC id={} addr=0x{:x} gsi_base={}",
                    ioapic_id,
                    ioapic_addr,
                    gsi_base
                );
            }
            2 if info.override_count < MAX_OVERRIDES => {
                let bus = unsafe { *madt_ptr.add(offset + 2) };
                let source = unsafe { *madt_ptr.add(offset + 3) };
                let gsi = unsafe { *(madt_ptr.add(offset + 4) as *const u32) };
                let flags = unsafe { *(madt_ptr.add(offset + 8) as *const u16) };
                info.overrides[info.override_count] = IrqOverride { source, gsi, flags };
                info.override_count += 1;

                log::trace!(
                    "IRQ override bus={} source={} gsi={} flags=0x{:x}",
                    bus,
                    source,
                    gsi,
                    flags
                );
            }
            _ => {}
        }

        offset += entry_len;
    }

    info
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect APIC hardware by walking the ACPI tables.
///
/// Returns `Some(ApicInfo)` if a valid MADT with at least one I/O APIC
/// was found.
///
/// # Safety
///
/// `rsdp_addr` must be a valid RSDP physical address. `phys_offset` must
/// be the physical memory offset used for virt-to-phys translation.
pub unsafe fn detect(rsdp_addr: u64, phys_offset: u64) -> Option<ApicInfo> {
    assert!(rsdp_addr != 0, "RSDP address must not be 0");
    assert!(phys_offset > 0, "physical memory offset must be > 0");

    let madt = unsafe { parse_rsdp(rsdp_addr, phys_offset) }?;
    let info = unsafe { parse_madt(madt) };
    debug_assert!(
        info.lapic_phys_base != 0,
        "MADT parsed with zero LAPIC base"
    );
    if info.ioapic_count == 0 {
        log::warn!("MADT: no I/O APIC found — using PIC fallback");
        return None;
    }
    log::info!(
        "APIC detected: LAPIC base=0x{:x}, {} I/O APIC(s)",
        info.lapic_phys_base,
        info.ioapic_count,
    );
    Some(info)
}

/// Initialise the APIC subsystem.
///
/// Returns `true` if APIC is now active and all legacy interrupts have
/// been routed.  Returns `false` on failure (caller should use PIC).
///
/// # Safety
///
/// Must be called after `detect()` succeeds and before interrupts are
/// enabled.  `phys_offset` must be the same value used in `detect()`.
pub unsafe fn init(info: &ApicInfo, phys_offset: u64) -> bool {
    assert!(info.lapic_phys_base != 0, "LAPIC base must be non-zero");
    assert!(info.ioapic_count >= 1, "at least one I/O APIC is required");
    assert!(info.ioapic_addrs[0] != 0, "first I/O APIC address is zero");

    let lapic_virt = (phys_offset + info.lapic_phys_base) as *mut u32;
    let lapic = unsafe { lapic::Lapic::new(lapic_virt) };

    let lapic_id = lapic.id();
    let lapic_ver = lapic.version();
    log::info!(
        "LAPIC id={} version={} base=0x{:x}",
        lapic_id,
        lapic_ver,
        info.lapic_phys_base
    );

    unsafe {
        lapic.enable(0xFF);
    }

    // Build a legacy IRQ → GSI lookup, applying MADT overrides.
    let irq_to_gsi = build_irq_gsi_map(&info.overrides, info.override_count);

    // Route timer + keyboard through the first I/O APIC.
    let ioapic_virt = (phys_offset + info.ioapic_addrs[0]) as *mut u32;
    let ioapic = unsafe { ioapic::IoApic::new(ioapic_virt, info.ioapic_gsi_bases[0]) };
    let gsi_base = ioapic.gsi_base();
    debug_assert!(gsi_base <= 256, "I/O APIC GSI base unusually large");

    let timer_gsi = irq_to_gsi[0];
    assert!(timer_gsi >= gsi_base);
    unsafe {
        ioapic.set_irq(
            (timer_gsi - gsi_base) as u8,
            crate::interrupts::InterruptIndex::Timer.as_u8(),
        );
    }
    log::info!("IOAPIC route: legacy IRQ0 → GSI {} → vector 32", timer_gsi);

    let kbd_gsi = irq_to_gsi[1];
    assert!(kbd_gsi >= gsi_base);
    unsafe {
        ioapic.set_irq(
            (kbd_gsi - gsi_base) as u8,
            crate::interrupts::InterruptIndex::Keyboard.as_u8(),
        );
    }
    log::info!("IOAPIC route: legacy IRQ1 → GSI {} → vector 33", kbd_gsi);

    // Store runtime state and set the fast-path EOI pointer.
    unsafe {
        LAPIC_EOI_PTR = lapic_virt.byte_add(lapic::LAPIC_EOI).cast::<u32>();
    }
    debug_assert!(
        unsafe { !LAPIC_EOI_PTR.is_null() },
        "LAPIC EOI pointer was not set"
    );
    *APIC.lock() = Some(ApicState { _lapic: lapic });

    log::info!("APIC subsystem active");
    true
}

/// Build a legacy IRQ → GSI mapping, applying MADT interrupt source
/// overrides.  Returns a 16-entry array indexed by legacy IRQ number.
pub fn build_irq_gsi_map(overrides: &[IrqOverride], override_count: usize) -> [u32; 16] {
    let mut irq_to_gsi = [0u32; 16];
    for (i, entry) in irq_to_gsi.iter_mut().enumerate() {
        *entry = i as u32;
    }
    for ov in overrides.iter().take(override_count) {
        if (ov.source as usize) < 16 {
            irq_to_gsi[ov.source as usize] = ov.gsi;
        }
    }
    irq_to_gsi
}

/// Returns `true` when the APIC is initialised and handling interrupts.
pub fn is_active() -> bool {
    APIC.lock().is_some()
}

/// Signal end-of-interrupt to the LAPIC.
///
/// # Safety
///
/// Must only be called from interrupt handlers when `is_active()`
/// returns `true`.
pub unsafe fn end_of_interrupt() {
    let ptr = unsafe { LAPIC_EOI_PTR };
    debug_assert!(!ptr.is_null(), "APIC EOI called before init");
    unsafe {
        core::ptr::write_volatile(ptr, 0);
    }
}
