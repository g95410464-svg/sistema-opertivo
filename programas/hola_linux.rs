// Programa real en formato ELF64 de Linux x86_64 con soporte de argumentos (argc/argv).
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C strip=symbols -C relocation-model=static -C link-arg=--image-base=0x400000 programas/hola_linux.rs -o programas/hola_linux.elf
#![no_std]
#![no_main]

use core::arch::{asm, naked_asm};
use core::panic::PanicInfo;

const SYS_WRITE: u64 = 1;
const SYS_GETPID: u64 = 39;
const SYS_EXIT: u64 = 60;
const SYS_GETPPID: u64 = 110;

#[inline(always)]
unsafe fn sys_write(fd: u64, buf: *const u8, len: usize) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") SYS_WRITE,
        in("rdi") fd,
        in("rsi") buf,
        in("rdx") len,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn sys_num(n: u64) -> u64 {
    let ret: u64;
    asm!(
        "syscall",
        in("rax") n,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn sys_exit(code: u64) -> ! {
    asm!(
        "syscall",
        in("rax") SYS_EXIT,
        in("rdi") code,
        options(noreturn)
    );
}

fn escribir(s: &[u8]) {
    unsafe {
        sys_write(1, s.as_ptr(), s.len());
    }
}

fn escribir_cstr(ptr: *const u8) {
    if ptr.is_null() {
        return;
    }
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 && len < 256 {
            len += 1;
        }
        sys_write(1, ptr, len);
    }
}

fn escribir_num(mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if n == 0 {
        escribir(b"0");
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    escribir(&buf[i..]);
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "mov rdi, [rsp]",
        "lea rsi, [rsp + 8]",
        "and rsp, -16",
        "call {inicio}",
        inicio = sym inicio,
    );
}

extern "sysv64" fn inicio(argc: u64, argv: *const *const u8) -> ! {
    let pid = unsafe { sys_num(SYS_GETPID) };
    let ppid = unsafe { sys_num(SYS_GETPPID) };

    escribir(b"\x1b[96m[hola ELF / Ring 3]\x1b[0m Hola");
    if argc > 1 {
        for i in 1..argc as usize {
            escribir(b" ");
            let arg = unsafe { *argv.add(i) };
            escribir_cstr(arg);
        }
    } else {
        escribir(b" mundo");
    }
    escribir(b"! (PID=");
    escribir_num(pid);
    escribir(b", PPID=");
    escribir_num(ppid);
    escribir(b")\n");

    unsafe { sys_exit(0) }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe { sys_exit(1) }
}
