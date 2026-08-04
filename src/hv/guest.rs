/// Machine code for the "hello world" guest. It just pokes two registers and
/// executes `hlt`, which causes a VM exit (HLT exiting is enabled). On resume
/// it jumps back to the `hlt` and exits again, so the hypervisor can count
/// intercepts and eventually shut itself down.
///
/// Source lives in `guest/hello.S`; the bytes below are the assembled flat
/// binary (committed so CI doesn't need an assembler):
///   mov edi, 0xDEADBEEF    bf ef be ad de
///   mov ebx, 0x12345678    bb 78 56 34 12
///   hlt                    f4
///   jmp -3                 eb fd
pub static GUEST_BIN: &[u8] = include_bytes!("../../guest/hello.bin");
