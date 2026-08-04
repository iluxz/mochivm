use core::arch::asm;

/// cpuid(leaf, subleaf) -> (eax, ecx, edx). EBX is deliberately not returned:
/// it's reserved by LLVM and can't be an asm operand, and nothing here needs it.
#[inline(always)]
pub unsafe fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32) {
    let mut eax: u32 = leaf;
    let mut ecx: u32 = subleaf;
    let mut edx: u32;
    asm!(
        "push rbx",
        "cpuid",
        "pop rbx",
        inout("eax") eax,
        inout("ecx") ecx,
        out("edx") edx,
        options(nostack)
    );
    (eax, ecx, edx)
}

#[inline(always)]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let mut lo: u32;
    let mut hi: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") lo,
        out("edx") hi,
        options(nomem, nostack, preserves_flags)
    );
    ((hi as u64) << 32) | lo as u64
}

#[inline(always)]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") value as u32,
        in("edx") (value >> 32) as u32,
        options(nomem, nostack, preserves_flags)
    );
}

#[inline(always)]
pub unsafe fn read_cr0() -> u64 {
    let v: u64;
    asm!("mov cr0, {0}", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline(always)]
pub unsafe fn read_cr3() -> u64 {
    let v: u64;
    asm!("mov cr3, {0}", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline(always)]
pub unsafe fn read_cr4() -> u64 {
    let v: u64;
    asm!("mov cr4, {0}", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

#[inline(always)]
pub unsafe fn write_cr0(v: u64) {
    asm!("mov {0}, cr0", in(reg) v, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn write_cr4(v: u64) {
    asm!("mov {0}, cr4", in(reg) v, options(nomem, nostack, preserves_flags));
}

macro_rules! read_seg {
    ($name:ident, $seg:tt) => {
        #[inline(always)]
        pub unsafe fn $name() -> u16 {
            let v: u16;
            asm!(
                concat!("mov {0:x}, ", stringify!($seg)),
                out(reg) v,
                options(nomem, nostack, preserves_flags)
            );
            v
        }
    };
}

read_seg!(read_cs, cs);
read_seg!(read_ss, ss);
read_seg!(read_ds, ds);
read_seg!(read_es, es);
read_seg!(read_fs, fs);
read_seg!(read_gs, gs);

#[inline(always)]
pub unsafe fn read_tr() -> u16 {
    let v: u16;
    asm!("str {0:x}", out(reg) v, options(nomem, nostack, preserves_flags));
    v
}

/// sgdt -> (limit, base)
#[inline(always)]
pub unsafe fn sgdt() -> (u16, u64) {
    let mut b = [0u8; 10];
    asm!("sgdt [{0}]", in(reg) b.as_mut_ptr(), options(nostack));
    let limit = u16::from_le_bytes([b[0], b[1]]);
    let base = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9]]);
    (limit, base)
}

/// sidt -> (limit, base)
#[inline(always)]
pub unsafe fn sidt() -> (u16, u64) {
    let mut b = [0u8; 10];
    asm!("sidt [{0}]", in(reg) b.as_mut_ptr(), options(nostack));
    let limit = u16::from_le_bytes([b[0], b[1]]);
    let base = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9]]);
    (limit, base)
}

/// Read the base address of a GDT entry at `offset` (used for the host TSS base).
#[inline(always)]
pub unsafe fn read_gdt_entry_base(gdt_base: u64, offset: u64) -> u64 {
    let mut desc = [0u8; 16];
    for i in 0..16u64 {
        core::ptr::read_volatile((gdt_base + offset + i) as *const u8);
        desc[i as usize] = core::ptr::read((gdt_base + offset + i) as *const u8);
    }
    let base = (desc[2] as u64)
        | ((desc[3] as u64) << 8)
        | ((desc[4] as u64) << 16)
        | ((desc[7] as u64) << 24)
        | ((desc[8] as u64) << 32)
        | ((desc[9] as u64) << 40)
        | ((desc[10] as u64) << 48)
        | ((desc[11] as u64) << 56);
    base
}
