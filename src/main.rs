#![no_std]
#![no_main]

pub mod hv;
pub mod serial;
pub mod x86;

/// System table pointer stashed at entry so we can call firmware (reset, alloc) later.
pub static mut SYSTEM_TABLE: usize = 0;

/// UEFI application entry point. The x86_64-unknown-uefi target links against the
/// symbol `efi_main` and uses the MS x64 calling convention, so `win64` is correct.
#[no_mangle]
pub extern "win64" fn efi_main(_image_handle: usize, system_table: usize) -> usize {
    unsafe {
        SYSTEM_TABLE = system_table;
    }

    serial::init();
    serial_println!("[mochivm] mochivm booted");
    serial_println!(
        "[mochivm] from-scratch type-2 hypervisor skeleton (vmx/vmcs + hello-world guest)"
    );

    unsafe {
        hv::init();
    }

    0
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("[mochivm] PANIC: {}", info.message());
    unsafe {
        hv::shutdown();
    }
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}
