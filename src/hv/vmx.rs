use crate::x86;

pub const CR4_VMXE: u64 = 1 << 13;

const IA32_FEATURE_CONTROL: u32 = 0x3A;
const IA32_VMX_BASIC: u32 = 0x480;
pub const IA32_VMX_PINBASED_CTLS: u32 = 0x481;
pub const IA32_VMX_PROCBASED_CTLS: u32 = 0x482;
pub const IA32_VMX_EXIT_CTLS: u32 = 0x483;
pub const IA32_VMX_ENTRY_CTLS: u32 = 0x484;
const IA32_VMX_CR0_FIXED0: u32 = 0x486;
const IA32_VMX_CR0_FIXED1: u32 = 0x487;
const IA32_VMX_CR4_FIXED0: u32 = 0x488;
const IA32_VMX_CR4_FIXED1: u32 = 0x489;
const IA32_VMX_SECONDARY_PROCBASED_CTLS: u32 = 0x48B;

pub unsafe fn vmx_supported() -> bool {
    let (_, ecx, _) = x86::cpuid(1, 0);
    (ecx & (1 << 5)) != 0
}

/// Unlock VMX in IA32_FEATURE_CONTROL if the BIOS left it unlocked.
pub unsafe fn feature_control() -> Result<(), &'static str> {
    let msr = x86::rdmsr(IA32_FEATURE_CONTROL);
    if msr & 1 != 0 {
        if msr & 2 == 0 {
            return Err("vmx locked and disabled by firmware");
        }
    } else {
        x86::wrmsr(IA32_FEATURE_CONTROL, msr | 3);
    }
    Ok(())
}

/// Force CR0/CR4 to satisfy the VMX fixed-bit requirements (this also sets
/// CR4.VMXE).
pub unsafe fn apply_fixed_bits() {
    let cr0_fixed0 = x86::rdmsr(IA32_VMX_CR0_FIXED0);
    let cr0_fixed1 = x86::rdmsr(IA32_VMX_CR0_FIXED1);
    let cr0 = (x86::read_cr0() | cr0_fixed0) & cr0_fixed1;
    x86::write_cr0(cr0);

    let cr4_fixed0 = x86::rdmsr(IA32_VMX_CR4_FIXED0);
    let cr4_fixed1 = x86::rdmsr(IA32_VMX_CR4_FIXED1);
    let cr4 = (x86::read_cr4() | cr4_fixed0) & cr4_fixed1;
    x86::write_cr4(cr4);
}

/// RFLAGS is captured immediately after each VMX instruction (which set CF on
/// failure; CF = bit 0 of RFLAGS), because rust inline asm has no flag-output
/// register on this target.
/// Enter VMX root operation on the current logical processor.
pub unsafe fn vmxon(region: *mut u8) -> Result<(), &'static str> {
    let revision = x86::rdmsr(IA32_VMX_BASIC) as u32;
    core::ptr::write_volatile(region as *mut u32, revision);

    let flags: u64;
    core::arch::asm!(
        "vmxon [{0}]",
        "pushfq",
        "pop rax",
        in(reg) region as u64,
        out("rax") flags,
    );
    if flags & 1 != 0 {
        return Err("vmxon failed (cf set)");
    }
    Ok(())
}

pub unsafe fn vmxoff() -> Result<(), ()> {
    let flags: u64;
    core::arch::asm!(
        "vmxoff",
        "pushfq",
        "pop rax",
        out("rax") flags,
    );
    if flags & 1 != 0 {
        return Err(());
    }
    Ok(())
}

pub unsafe fn vmclear(region: *mut u8) -> Result<(), ()> {
    let flags: u64;
    core::arch::asm!(
        "vmclear [{0}]",
        "pushfq",
        "pop rax",
        in(reg) region as u64,
        out("rax") flags,
    );
    if flags & 1 != 0 {
        return Err(());
    }
    Ok(())
}

pub unsafe fn vmptrld(region: *mut u8) -> Result<(), ()> {
    let flags: u64;
    core::arch::asm!(
        "vmptrld [{0}]",
        "pushfq",
        "pop rax",
        in(reg) region as u64,
        out("rax") flags,
    );
    if flags & 1 != 0 {
        return Err(());
    }
    Ok(())
}

pub unsafe fn vmwrite(field: u32, value: u64) -> Result<(), ()> {
    let flags: u64;
    core::arch::asm!(
        "vmwrite rcx, rdx",
        "pushfq",
        "pop rax",
        in("rcx") field,
        in("rdx") value,
        out("rax") flags,
    );
    if flags & 1 != 0 {
        return Err(());
    }
    Ok(())
}

pub unsafe fn vmread(field: u32) -> u64 {
    let value: u64;
    core::arch::asm!(
        "vmread rax, rcx",
        out("rax") value,
        in("rcx") field,
        options(nostack)
    );
    value
}

/// Compute a control value honoring the allowed-1 / default-1 masks of a
/// capability MSR (0x481-0x484). Low 32 = default-1 bits, high 32 = allowed-1.
pub unsafe fn calc_ctrl(msr: u32, desired: u32) -> u32 {
    let caps = x86::rdmsr(msr);
    (desired | caps as u32) & (caps >> 32) as u32
}

/// Secondary controls have no default-1 mask, only an allowed-1 mask.
pub unsafe fn calc_secondary(desired: u32) -> u32 {
    let caps = x86::rdmsr(IA32_VMX_SECONDARY_PROCBASED_CTLS);
    desired & (caps >> 32) as u32
}
