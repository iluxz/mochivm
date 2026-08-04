# Source for the hello-world guest. The assembled flat binary is committed as
# `guest/hello.bin` (see src/hv/guest.rs for the byte layout). Rebuild with:
#   as -o guest.o hello.S && objcopy -O binary guest.o hello.bin
.intel_syntax noprefix
.section .text
.global _start
_start:
    mov edi, 0xDEADBEEF
    mov ebx, 0x12345678
loop:
    hlt
    jmp loop
