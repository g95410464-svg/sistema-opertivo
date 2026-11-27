// Filtro Unix en Ring 3 (Linux ELF64): lee stdin hasta EOF y convierte todo a MAYUSCULAS
// preservando secuencias de color ANSI (\x1b[...m).
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C strip=symbols -C relocation-model=static -C link-arg=--image-base=0x400000 programas/upper.rs -o programas/upper.elf
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 60;

#[inline(always)]
unsafe fn syscall3(num: u64, a1: u64, a2: u64, a3: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
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

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut buf = [0u8; 256];
    let mut en_ansi: u8 = 0; // 0 = normal, 1 = vio ESC, 2 = dentro de CSI
    loop {
        let n = unsafe { syscall3(SYS_READ, 0, buf.as_mut_ptr() as u64, buf.len() as u64) };
        if n <= 0 {
            break;
        }
        let slice = &mut buf[..n as usize];
        for b in slice.iter_mut() {
            match en_ansi {
                0 => {
                    if *b == 0x1b {
                        en_ansi = 1;
                    } else if *b >= b'a' && *b <= b'z' {
                        *b -= 32;
                    }
                }
                1 => {
                    en_ansi = if *b == b'[' { 2 } else { 0 };
                }
                _ => {
                    if !b.is_ascii_digit() && *b != b';' {
                        en_ansi = 0;
                    }
                }
            }
        }
        unsafe {
            syscall3(SYS_WRITE, 1, slice.as_ptr() as u64, slice.len() as u64);
        }
    }
    unsafe { sys_exit(0) }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe { sys_exit(1) }
}
