use core::fmt;

const COM1: u16 = 0x3F8;

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
}

/// Init a plain 16550 UART on COM1 at 115200 8N1.
pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // no interrupts
        outb(COM1 + 3, 0x80); // DLAB on
        outb(COM1 + 0, 0x01); // divisor low (115200)
        outb(COM1 + 1, 0x00); // divisor high
        outb(COM1 + 3, 0x03); // 8N1
        outb(COM1 + 2, 0xC7); // FIFO enable/clear
        outb(COM1 + 4, 0x0B); // DTR+RTS
    }
}

fn put_char_raw(c: u8) {
    unsafe {
        while inb(COM1 + 5) & 0x20 == 0 {}
        outb(COM1, c);
    }
}

pub fn put_char(c: u8) {
    if c == b'\n' {
        put_char_raw(b'\r');
    }
    put_char_raw(c);
}

pub fn write(s: &str) {
    for &b in s.as_bytes() {
        put_char(b);
    }
}

/// fmt::Write shim so `serial_println!` can use format_args.
pub struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! serial_println {
    () => {
        $crate::serial::write("\n");
    };
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(
            &mut $crate::serial::Writer,
            core::format_args!($($arg)*),
        );
        $crate::serial::write("\n");
    }};
}
