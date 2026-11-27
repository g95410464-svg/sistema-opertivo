// Programa real en formato ELF64 de Linux x86_64.
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C relocation-model=static -C link-arg=--image-base=0x400000 programas/hola_linux.rs -o programas/hola_linux.elf
// Este mismo binario corre tambien en un Linux real x86_64 (Debian/Ubuntu/Alpine).
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_WRITE: u64 = 1;
const SYS_GETPID: u64 = 39;
const SYS_EXIT: u64 = 60;

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
unsafe fn sys_getpid() -> u64 {
    let pid: u64;
    asm!(
        "syscall",
        in("rax") SYS_GETPID,
        lateout("rax") pid,
        out("rcx") _,
        out("r11") _,
    );
    pid
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

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    escribir(b"[ELF Linux / Ring 3] Hola desde modo usuario!\n");
    let pid = unsafe { sys_getpid() };
    escribir(b"[ELF Linux / Ring 3] Mi PID (syscall getpid) = ");
    escribir_num(pid);
    escribir(b"\n");
    escribir(b"[ELF Linux / Ring 3] Saliendo con syscall exit(0)...\n");
    unsafe { sys_exit(0) }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe { sys_exit(1) }
}
