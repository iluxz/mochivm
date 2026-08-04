pub mod efi;
pub mod guest;
pub mod vmcs;
pub mod vmx;

use crate::serial_println;

/// Full hypervisor bring-up. On success this launches the guest and only
/// returns via VM exit handling (which eventually shuts VMX back down).
pub unsafe fn init() {
    serial_println!("[mochivm] probing for vmx...");
    if !vmx::vmx_supported() {
        serial_println!("[mochivm] error: vmx not supported by this cpu");
        return;
    }

    if let Err(e) = vmx::feature_control() {
        serial_println!("[mochivm] error: feature control: {}", e);
        return;
    }

    vmx::apply_fixed_bits();

    match vmcs::setup() {
        Ok(()) => {
            serial_println!("[mochivm] launching guest...");
            vmcs::enter_guest();
        }
        Err(e) => {
            serial_println!("[mochivm] error: vmcs setup: {}", e);
        }
    }
}

pub unsafe fn shutdown() {
    vmcs::teardown();
}
