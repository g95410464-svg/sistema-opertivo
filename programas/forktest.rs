// Prueba real de fork() + aislamiento de memoria CR3 + wait4() en Ring 3 (Linux x86_64 ELF).
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C strip=symbols -C relocation-model=static -C link-arg=--image-base=0x400000 programas/forktest.rs -o programas/forktest.elf
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_WRITE: u64 = 1;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_GETPPID: u64 = 110;

#[inline(always)]
unsafe fn sys_write(fd: u64, buf: &[u8]) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") SYS_WRITE,
        in("rdi") fd,
        in("rsi") buf.as_ptr(),
        in("rdx") buf.len(),
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn sys0(n: u64) -> isize {
    let ret: isize;
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
unsafe fn sys_wait4(pid: i64, status: *mut i32) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") SYS_WAIT4,
        in("rdi") pid as u64,
        in("rsi") status as u64,
        in("rdx") 0u64,
        in("r10") 0u64,
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
        sys_write(1, s);
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
    let mut secreto: u64 = 42;
    let p_secreto: *mut u64 = &mut secreto;

    let hijo = unsafe { sys0(SYS_FORK) };
    if hijo < 0 {
        escribir(b"[forktest] Error en fork!\n");
        unsafe { sys_exit(1) }
    }

    if hijo == 0 {
        // Proceso hijo: modifica la variable en su propia tabla de paginas CR3.
        unsafe { p_secreto.write_volatile(999) };
        let val = unsafe { p_secreto.read_volatile() };
        let pid = unsafe { sys0(SYS_GETPID) as u64 };
        let ppid = unsafe { sys0(SYS_GETPPID) as u64 };
        escribir(b"\x1b[93m[hijo PID=");
        escribir_num(pid);
        escribir(b", PPID=");
        escribir_num(ppid);
        escribir(b"]\x1b[0m cambie secreto = ");
        escribir_num(val);
        escribir(b" y salgo con codigo 7\n");
        unsafe { sys_exit(7) }
    } else {
        // Proceso padre: espera al hijo y verifica que su memoria NO se altero.
        let mut status: i32 = 0;
        let esperado = unsafe { sys_wait4(hijo as i64, &mut status) };
        let codigo_hijo = ((status >> 8) & 0xff) as u64;
        let val = unsafe { p_secreto.read_volatile() };
        let pid = unsafe { sys0(SYS_GETPID) as u64 };
        escribir(b"\x1b[96m[padre PID=");
        escribir_num(pid);
        escribir(b"]\x1b[0m hijo ");
        escribir_num(esperado as u64);
        escribir(b" salio con ");
        escribir_num(codigo_hijo);
        escribir(b" | secreto=");
        escribir_num(val);
        escribir(b" (CR3 aislado OK!)\n");
        unsafe { sys_exit(0) }
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe { sys_exit(1) }
}
