use crate::hv::{efi, vmx};
use crate::serial_println;
use crate::x86;
use core::sync::atomic::{AtomicU64, Ordering};

// --- VMCS field encodings (Intel SDM, appendix B) ---

// control fields (32-bit)
const PIN_BASED_CTLS: u32 = 0x4000;
const PROC_BASED_CTLS: u32 = 0x4002;
const EXC_BITMAP: u32 = 0x4004;
const PF_ERROR_MASK: u32 = 0x4006;
const PF_ERROR_MATCH: u32 = 0x4008;
const CR3_TARGET_COUNT: u32 = 0x400A;
const VM_EXIT_CTLS: u32 = 0x400C;
const VM_EXIT_MSR_STORE_COUNT: u32 = 0x400E;
const VM_EXIT_MSR_LOAD_COUNT: u32 = 0x4010;
const VM_ENTRY_CTLS: u32 = 0x4012;
const VM_ENTRY_MSR_LOAD_COUNT: u32 = 0x4014;
const VM_ENTRY_INTR_INFO: u32 = 0x4016;
const VM_ENTRY_EXC_ERR: u32 = 0x4018;
const VM_ENTRY_INSTR_LEN: u32 = 0x401A;
const SECONDARY_PROC_BASED_CTLS: u32 = 0x401E;

// read-only data fields
const EXIT_REASON: u32 = 0x4402;
const EXIT_QUALIFICATION: u32 = 0x6400;

// host-state fields
const HOST_CR0: u32 = 0x6C00;
const HOST_CR3: u32 = 0x6C02;
const HOST_CR4: u32 = 0x6C04;
const HOST_FS_BASE: u32 = 0x6C06;
const HOST_GS_BASE: u32 = 0x6C08;
const HOST_TR_BASE: u32 = 0x6C0A;
const HOST_GDTR_BASE: u32 = 0x6C0C;
const HOST_IDTR_BASE: u32 = 0x6C0E;
const HOST_SYSENTER_ESP: u32 = 0x6C10;
const HOST_SYSENTER_EIP: u32 = 0x6C12;
const HOST_RSP: u32 = 0x6C14;
const HOST_RIP: u32 = 0x6C16;
const HOST_TR_SEL: u32 = 0x0C00;
const HOST_FS_SEL: u32 = 0x0C02;
const HOST_GS_SEL: u32 = 0x0C04;
const HOST_SS_SEL: u32 = 0x0C06;
const HOST_DS_SEL: u32 = 0x0C08;
const HOST_ES_SEL: u32 = 0x0C0A;
const HOST_SYSENTER_CS: u32 = 0x4C00;
const HOST_IA32_EFER: u32 = 0x2C02;

// guest-state fields
const GUEST_CR0: u32 = 0x6800;
const GUEST_CR3: u32 = 0x6802;
const GUEST_CR4: u32 = 0x6804;
const GUEST_ES_BASE: u32 = 0x6806;
const GUEST_CS_BASE: u32 = 0x6808;
const GUEST_SS_BASE: u32 = 0x680A;
const GUEST_DS_BASE: u32 = 0x680C;
const GUEST_FS_BASE: u32 = 0x680E;
const GUEST_GS_BASE: u32 = 0x6810;
const GUEST_LDTR_BASE: u32 = 0x6812;
const GUEST_TR_BASE: u32 = 0x6814;
const GUEST_GDTR_BASE: u32 = 0x6816;
const GUEST_IDTR_BASE: u32 = 0x6818;
const GUEST_DR7: u32 = 0x681A;
const GUEST_RSP: u32 = 0x681C;
const GUEST_RIP: u32 = 0x681E;
const GUEST_RFLAGS: u32 = 0x6820;
const GUEST_PENDING_DBG_EXC: u32 = 0x6822;
const GUEST_SYSENTER_ESP: u32 = 0x6824;
const GUEST_SYSENTER_EIP: u32 = 0x6826;

const GUEST_ES_SEL: u32 = 0x0800;
const GUEST_CS_SEL: u32 = 0x0802;
const GUEST_SS_SEL: u32 = 0x0804;
const GUEST_DS_SEL: u32 = 0x0806;
const GUEST_FS_SEL: u32 = 0x0808;
const GUEST_GS_SEL: u32 = 0x080A;
const GUEST_LDTR_SEL: u32 = 0x080C;
const GUEST_TR_SEL: u32 = 0x080E;
const GUEST_SYSENTER_CS: u32 = 0x0820;

const GUEST_ES_LIMIT: u32 = 0x4800;
const GUEST_CS_LIMIT: u32 = 0x4802;
const GUEST_SS_LIMIT: u32 = 0x4804;
const GUEST_DS_LIMIT: u32 = 0x4806;
const GUEST_FS_LIMIT: u32 = 0x4808;
const GUEST_GS_LIMIT: u32 = 0x480A;
const GUEST_LDTR_LIMIT: u32 = 0x480C;
const GUEST_TR_LIMIT: u32 = 0x480E;
const GUEST_GDTR_LIMIT: u32 = 0x4810;
const GUEST_IDTR_LIMIT: u32 = 0x4812;

const GUEST_ES_AR: u32 = 0x4814;
const GUEST_CS_AR: u32 = 0x4816;
const GUEST_SS_AR: u32 = 0x4818;
const GUEST_DS_AR: u32 = 0x481A;
const GUEST_FS_AR: u32 = 0x481C;
const GUEST_GS_AR: u32 = 0x481E;
const GUEST_LDTR_AR: u32 = 0x4820;
const GUEST_TR_AR: u32 = 0x4822;

const GUEST_IA32_EFER: u32 = 0x2806;
const GUEST_VMCS_LINK_PTR: u32 = 0x2800;

// control bits
const HLT_EXITING: u32 = 1 << 7;
const HOST_LONG_MODE: u32 = 1 << 9;
const GUEST_LONG_MODE: u32 = 1 << 9;
const LOAD_IA32_EFER_ENTRY: u32 = 1 << 2;
const LOAD_IA32_EFER_EXIT: u32 = 1 << 3;
const EXIT_REASON_HLT: u64 = 12;

const MAX_HLT_EXITS: u64 = 5;

// segment access rights for a 64-bit flat segment layout
const AR_LONG_CODE: u32 = 0xA09B; // P=1 DPL=0 S=1 type=B(exec/read,acc) G=1 L=1
const AR_DATA: u32 = 0xC093; // P=1 DPL=0 S=1 type=3(read/write,acc) G=1 B=1
const AR_UNUSABLE: u32 = 0x10000;
const AR_TSS64_BUSY: u32 = 0x8B; // P=1 DPL=0 S=0 type=B(64-bit TSS, busy)

static mut VMXON_REGION: u64 = 0;
static mut VMCS_REGION: u64 = 0;
static mut VMX_ACTIVE: bool = false;

static EXIT_COUNT: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------

pub unsafe fn setup() -> Result<(), &'static str> {
    let vmxon_region = match efi::allocate_page() {
        Ok(p) => p,
        Err(e) => return Err(e),
    };
    if let Err(e) = vmx::vmxon(vmxon_region) {
        return Err(e);
    }
    VMXON_REGION = vmxon_region as u64;

    let vmcs_region = match efi::allocate_page() {
        Ok(p) => p,
        Err(e) => {
            vmxoff_and_clear();
            return Err(e);
        }
    };
    core::ptr::write_volatile(vmcs_region as *mut u32, x86::rdmsr(0x480) as u32);
    if vmx::vmclear(vmcs_region).is_err() {
        vmxoff_and_clear();
        return Err("vmclear failed");
    }
    if vmx::vmptrld(vmcs_region).is_err() {
        vmxoff_and_clear();
        return Err("vmptrld failed");
    }
    VMCS_REGION = vmcs_region as u64;

    let host_stack = match efi::allocate_pages(4) {
        Ok(p) => p,
        Err(e) => {
            vmxoff_and_clear();
            return Err(e);
        }
    };
    let guest_code = match efi::allocate_page() {
        Ok(p) => p,
        Err(e) => {
            vmxoff_and_clear();
            return Err(e);
        }
    };
    let guest_stack = match efi::allocate_page() {
        Ok(p) => p,
        Err(e) => {
            vmxoff_and_clear();
            return Err(e);
        }
    };

    core::ptr::copy_nonoverlapping(
        crate::hv::guest::GUEST_BIN.as_ptr(),
        guest_code,
        crate::hv::guest::GUEST_BIN.len(),
    );

    write_state(host_stack as u64, guest_code as u64, guest_stack as u64)?;

    VMX_ACTIVE = true;
    serial_println!("[mochivm] vmcs ready");
    Ok(())
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn enter_guest() -> ! {
    core::arch::naked_asm!("vmlaunch", "1:", "cli", "hlt", "jmp 1b",);
}

/// VM-exit handler entry. The CPU jumps to this (HOST_RIP) on every exit with
/// the host stack already switched by hardware.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn host_entry() -> ! {
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rbx",
        "push rbp",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rax", // alignment pad so rsp is 16-aligned before the call
        "call {handler}",
        "add rsp, 8", // drop the pad
        "test al, al",
        "jnz 2f",
        // resume the guest
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rbp",
        "pop rbx",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "vmresume",
        "1:",
        "cli",
        "hlt",
        "jmp 1b",
        // shutdown path: handler already did vmclear + vmxoff
        "2:",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rbp",
        "pop rbx",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "3:",
        "cli",
        "hlt",
        "jmp 3b",
        handler = sym exit_handler,
    );
}

/// C-side of the exit handler. Returns 0 to resume the guest, 1 to stop.
unsafe extern "sysv64" fn exit_handler() -> u8 {
    let reason = vmx::vmread(EXIT_REASON);
    if reason == EXIT_REASON_HLT {
        let count = EXIT_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
        serial_println!("[mochivm] guest hlt #{}", count);
        let rip = vmx::vmread(GUEST_RIP);
        let _ = vmx::vmwrite(GUEST_RIP, rip + 1);
        if count >= MAX_HLT_EXITS {
            serial_println!("[mochivm] guest halted {} times, vmxoff", count);
            teardown();
            return 1;
        }
        0
    } else {
        let qual = vmx::vmread(EXIT_QUALIFICATION);
        serial_println!(
            "[mochivm] unexpected exit reason {} qual 0x{:x}, stopping",
            reason,
            qual
        );
        teardown();
        1
    }
}

pub unsafe fn teardown() {
    if !VMX_ACTIVE {
        return;
    }
    let _ = vmx::vmclear(VMCS_REGION as *mut u8);
    let _ = vmx::vmxoff();
    let cr4 = x86::read_cr4();
    x86::write_cr4(cr4 & !vmx::CR4_VMXE);
    VMX_ACTIVE = false;
}

pub unsafe fn vmx_active() -> bool {
    VMX_ACTIVE
}

// ---------------------------------------------------------------------------

unsafe fn vmxoff_and_clear() {
    if VMXON_REGION != 0 {
        let _ = vmx::vmxoff();
        let cr4 = x86::read_cr4();
        x86::write_cr4(cr4 & !vmx::CR4_VMXE);
        VMXON_REGION = 0;
    }
}

unsafe fn write_state(
    host_stack: u64,
    guest_code: u64,
    guest_stack: u64,
) -> Result<(), &'static str> {
    let cr0 = x86::read_cr0();
    let cr3 = x86::read_cr3();
    let cr4 = x86::read_cr4();
    let efer = x86::rdmsr(0xC0000080);

    let (gdt_limit, gdt_base) = x86::sgdt();
    let (idt_limit, idt_base) = x86::sidt();
    let tr_sel = x86::read_tr() as u64;
    let tr_base = x86::read_gdt_entry_base(gdt_base, tr_sel);

    // controls
    write_ctrl(
        PIN_BASED_CTLS,
        vmx::calc_ctrl(vmx::IA32_VMX_PINBASED_CTLS, 0),
    );
    write_ctrl(
        PROC_BASED_CTLS,
        vmx::calc_ctrl(vmx::IA32_VMX_PROCBASED_CTLS, HLT_EXITING),
    );
    write_ctrl(SECONDARY_PROC_BASED_CTLS, vmx::calc_secondary(0));
    write_ctrl(EXC_BITMAP, 0);
    write_ctrl(PF_ERROR_MASK, 0);
    write_ctrl(PF_ERROR_MATCH, 0);
    write_ctrl(CR3_TARGET_COUNT, 0);
    write_ctrl(
        VM_EXIT_CTLS,
        vmx::calc_ctrl(
            vmx::IA32_VMX_EXIT_CTLS,
            HOST_LONG_MODE | LOAD_IA32_EFER_EXIT,
        ),
    );
    write_ctrl(
        VM_ENTRY_CTLS,
        vmx::calc_ctrl(
            vmx::IA32_VMX_ENTRY_CTLS,
            GUEST_LONG_MODE | LOAD_IA32_EFER_ENTRY,
        ),
    );
    write_ctrl(VM_EXIT_MSR_STORE_COUNT, 0);
    write_ctrl(VM_EXIT_MSR_LOAD_COUNT, 0);
    write_ctrl(VM_ENTRY_MSR_LOAD_COUNT, 0);
    write_ctrl(VM_ENTRY_INTR_INFO, 0);
    write_ctrl(VM_ENTRY_EXC_ERR, 0);
    write_ctrl(VM_ENTRY_INSTR_LEN, 0);

    // guest: general
    write64(GUEST_VMCS_LINK_PTR, 0xFFFF_FFFF_FFFF_FFFF);
    write64(GUEST_CR0, cr0);
    write64(GUEST_CR3, cr3);
    write64(GUEST_CR4, cr4);
    write64(GUEST_IA32_EFER, efer);
    write64(GUEST_RSP, guest_stack + 0x1000);
    write64(GUEST_RIP, guest_code);
    write64(GUEST_RFLAGS, 0x2);
    write64(GUEST_DR7, 0x400);
    write64(GUEST_PENDING_DBG_EXC, 0);
    write64(GUEST_SYSENTER_ESP, 0);
    write64(GUEST_SYSENTER_EIP, 0);
    write32(GUEST_SYSENTER_CS, 0);

    // guest: segments
    write32(GUEST_CS_SEL, 0x08);
    write32(GUEST_SS_SEL, 0x10);
    write32(GUEST_DS_SEL, 0x10);
    write32(GUEST_ES_SEL, 0x10);
    write32(GUEST_FS_SEL, 0x10);
    write32(GUEST_GS_SEL, 0x10);
    write32(GUEST_LDTR_SEL, 0);
    write32(GUEST_TR_SEL, tr_sel as u32);

    write64(GUEST_CS_BASE, 0);
    write64(GUEST_SS_BASE, 0);
    write64(GUEST_DS_BASE, 0);
    write64(GUEST_ES_BASE, 0);
    write64(GUEST_FS_BASE, 0);
    write64(GUEST_GS_BASE, 0);
    write64(GUEST_LDTR_BASE, 0);
    write64(GUEST_TR_BASE, tr_base);

    write64(GUEST_GDTR_BASE, gdt_base);
    write64(GUEST_IDTR_BASE, idt_base);
    write32(GUEST_GDTR_LIMIT, gdt_limit as u32);
    write32(GUEST_IDTR_LIMIT, idt_limit as u32);

    write32(GUEST_CS_LIMIT, 0xFFFF_FFFF);
    write32(GUEST_SS_LIMIT, 0xFFFF_FFFF);
    write32(GUEST_DS_LIMIT, 0xFFFF_FFFF);
    write32(GUEST_ES_LIMIT, 0xFFFF_FFFF);
    write32(GUEST_FS_LIMIT, 0xFFFF_FFFF);
    write32(GUEST_GS_LIMIT, 0xFFFF_FFFF);
    write32(GUEST_LDTR_LIMIT, 0);
    write32(GUEST_TR_LIMIT, 0x67);

    write32(GUEST_CS_AR, AR_LONG_CODE);
    write32(GUEST_SS_AR, AR_DATA);
    write32(GUEST_DS_AR, AR_DATA);
    write32(GUEST_ES_AR, AR_DATA);
    write32(GUEST_FS_AR, AR_DATA);
    write32(GUEST_GS_AR, AR_DATA);
    write32(GUEST_LDTR_AR, AR_UNUSABLE);
    write32(GUEST_TR_AR, AR_TSS64_BUSY);

    // host state
    write64(HOST_CR0, cr0);
    write64(HOST_CR3, cr3);
    write64(HOST_CR4, cr4);
    write64(HOST_RSP, host_stack + 0x4000);
    write64(HOST_RIP, host_entry as *const () as usize as u64);
    write64(HOST_FS_BASE, 0);
    write64(HOST_GS_BASE, 0);
    write64(HOST_TR_BASE, tr_base);
    write64(HOST_GDTR_BASE, gdt_base);
    write64(HOST_IDTR_BASE, idt_base);
    write64(HOST_SYSENTER_ESP, 0);
    write64(HOST_SYSENTER_EIP, 0);
    write64(HOST_IA32_EFER, efer);

    write32(HOST_TR_SEL, tr_sel as u32);
    write32(HOST_SS_SEL, x86::read_ss() as u32);
    write32(HOST_DS_SEL, x86::read_ds() as u32);
    write32(HOST_ES_SEL, x86::read_es() as u32);
    write32(HOST_FS_SEL, x86::read_fs() as u32);
    write32(HOST_GS_SEL, x86::read_gs() as u32);
    write32(HOST_SYSENTER_CS, 0);

    Ok(())
}

unsafe fn write_ctrl(field: u32, value: u32) {
    vmx::vmwrite(field, value as u64).expect("vmwrite ctrl failed");
}

unsafe fn write32(field: u32, value: u32) {
    vmx::vmwrite(field, value as u64).expect("vmwrite32 failed");
}

unsafe fn write64(field: u32, value: u64) {
    vmx::vmwrite(field, value).expect("vmwrite64 failed");
}
