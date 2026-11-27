// Capa de compatibilidad con Linux x86_64: SYSCALL/SYSRET y salto a modo usuario (Ring 3).
use crate::{fs, gdt, memoria, print, println, teclado};
use core::arch::{asm, naked_asm};
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask, Star};
use x86_64::registers::rflags::RFlags;
use x86_64::structures::paging::{Page, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

// Numeros oficiales de syscalls de Linux x86_64.
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_BRK: u64 = 12;
const SYS_GETPID: u64 = 39;
const SYS_EXIT: u64 = 60;
const SYS_UNAME: u64 = 63;
const SYS_GETDENTS64: u64 = 217;
const SYS_EXIT_GROUP: u64 = 231;
const SYS_OPENAT: u64 = 257;

// Codigos de error POSIX/Linux (negativos).
const EFAULT: i64 = -14;
const ENOSYS: i64 = -38;

const LIMITE_USUARIO: u64 = 0x0000_8000_0000_0000;
const MAX_BRK_CRECIMIENTO: u64 = 16 * 1024 * 1024; // 16 MiB max de heap de usuario

#[repr(C, align(16))]
struct PilaSyscall([u8; 4096 * 5]);

static mut PILA_SYSCALL: PilaSyscall = PilaSyscall([0; 4096 * 5]);
static mut KERNEL_SYSCALL_RSP: u64 = 0;
static mut USER_RSP_TMP: u64 = 0;
static mut RETORNO_KERNEL_RSP: u64 = 0;
static mut CODIGO_SALIDA: i64 = 0;
static mut BRK_INICIO: u64 = 0;
static mut BRK_ACTUAL: u64 = 0;

#[repr(C)]
struct ContextoSyscall {
    rax: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    r10: u64,
    r8: u64,
    r9: u64,
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    user_rip: u64,
    user_rflags: u64,
    user_rsp: u64,
}

/// Configura los registros MSR del CPU para habilitar la instruccion `syscall`.
pub fn init() {
    let sel = gdt::selectores();
    unsafe {
        let tope = (&raw const PILA_SYSCALL as u64) + core::mem::size_of::<PilaSyscall>() as u64;
        KERNEL_SYSCALL_RSP = tope;

        Efer::update(|flags| flags.insert(EferFlags::SYSTEM_CALL_EXTENSIONS));

        Star::write(
            sel.codigo_usuario,
            sel.datos_usuario,
            sel.codigo_kernel,
            sel.datos_kernel,
        )
        .expect("Selectores GDT invalidos para STAR");

        LStar::write(VirtAddr::new(entrada_syscall as *const () as u64));
        SFMask::write(RFlags::INTERRUPT_FLAG);
    }
}

/// Ejecuta un programa en modo usuario (Ring 3) y devuelve su codigo de salida de `sys_exit`.
pub fn ejecutar_en_usuario(punto_entrada: u64, pila_usuario: u64, brk_inicio: u64) -> i64 {
    let sel = gdt::selectores();
    unsafe {
        BRK_INICIO = brk_inicio;
        BRK_ACTUAL = brk_inicio;
        entrar_usuario(
            punto_entrada,
            pila_usuario,
            sel.codigo_usuario.0 as u64,
            sel.datos_usuario.0 as u64,
        )
    }
}

#[unsafe(naked)]
unsafe extern "sysv64" fn entrar_usuario(
    _entry: u64,
    _user_rsp: u64,
    _cs_user: u64,
    _ss_user: u64,
) -> i64 {
    naked_asm!(
        "pushfq",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rip + {retorno}], rsp",

        // Marco para iretq: SS, RSP, RFLAGS (IF=1), CS, RIP
        "push rcx",
        "push rsi",
        "push 0x202",
        "push rdx",
        "push rdi",

        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rdi, rdi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",
        "iretq",
        retorno = sym RETORNO_KERNEL_RSP,
    );
}

unsafe fn salir_de_usuario(codigo: i64) -> ! {
    CODIGO_SALIDA = codigo;
    asm!(
        "mov rsp, [rip + {retorno}]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "popfq",
        "mov rax, [rip + {codigo}]",
        "ret",
        retorno = sym RETORNO_KERNEL_RSP,
        codigo = sym CODIGO_SALIDA,
        options(noreturn)
    );
}

#[unsafe(naked)]
unsafe extern "C" fn entrada_syscall() {
    naked_asm!(
        "mov [rip + {user_rsp}], rsp",
        "mov rsp, [rip + {kernel_rsp}]",

        "push qword ptr [rip + {user_rsp}]",
        "push r11",
        "push rcx",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push rbp",
        "push rbx",
        "push r9",
        "push r8",
        "push r10",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rax",

        "mov rdi, rsp",
        "call {despachar}",

        "pop rax",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop r10",
        "pop r8",
        "pop r9",
        "pop rbx",
        "pop rbp",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "pop rcx",
        "pop r11",
        "pop rsp",
        "sysretq",
        user_rsp = sym USER_RSP_TMP,
        kernel_rsp = sym KERNEL_SYSCALL_RSP,
        despachar = sym despachar_syscall,
    );
}

fn validar_rango_usuario(ptr: u64, len: u64) -> bool {
    ptr >= 0x1000 && len <= 1024 * 1024 && ptr.saturating_add(len) < LIMITE_USUARIO
}

fn leer_cstr_usuario(ptr: u64) -> Option<&'static str> {
    if !validar_rango_usuario(ptr, 1) {
        return None;
    }
    let mut len = 0usize;
    while len < 256 {
        let dir = ptr + len as u64;
        if dir >= LIMITE_USUARIO {
            return None;
        }
        let b = unsafe { *(dir as *const u8) };
        if b == 0 {
            break;
        }
        len += 1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(bytes).ok()
}

extern "sysv64" fn despachar_syscall(ctx: &mut ContextoSyscall) {
    let res: i64 = match ctx.rax {
        SYS_READ => sys_read(ctx.rdi as usize, ctx.rsi, ctx.rdx),
        SYS_WRITE => sys_write(ctx.rdi as usize, ctx.rsi, ctx.rdx),
        SYS_OPEN => sys_openat(ctx.rdi, ctx.rsi),
        SYS_OPENAT => sys_openat(ctx.rsi, ctx.rdx),
        SYS_CLOSE => fs::cerrar(ctx.rdi as usize),
        SYS_BRK => sys_brk(ctx.rdi) as i64,
        SYS_GETPID => 1,
        SYS_UNAME => sys_uname(ctx.rdi),
        SYS_GETDENTS64 => sys_getdents64(ctx.rdi as usize, ctx.rsi, ctx.rdx),
        SYS_EXIT | SYS_EXIT_GROUP => unsafe {
            salir_de_usuario(ctx.rdi as i64);
        },
        otra => {
            println!("[neo_ws] Syscall Linux no implementada: {}", otra);
            ENOSYS
        }
    };
    ctx.rax = res as u64;
}

fn sys_read(fd: usize, buf: u64, len: u64) -> i64 {
    if len == 0 {
        return 0;
    }
    if !validar_rango_usuario(buf, len) {
        return EFAULT;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len as usize) };
    if fd == 0 {
        teclado::leer_linea(dst) as i64
    } else {
        fs::leer(fd, dst)
    }
}

fn sys_write(fd: usize, buf: u64, len: u64) -> i64 {
    if len == 0 {
        return 0;
    }
    if !validar_rango_usuario(buf, len) {
        return EFAULT;
    }
    let bytes = unsafe { core::slice::from_raw_parts(buf as *const u8, len as usize) };
    if fd == 1 || fd == 2 {
        let texto = core::str::from_utf8(bytes).unwrap_or("<bytes no UTF-8>");
        print!("{}", texto);
        len as i64
    } else {
        fs::escribir(fd, bytes)
    }
}

fn sys_openat(ruta_ptr: u64, flags: u64) -> i64 {
    let Some(ruta) = leer_cstr_usuario(ruta_ptr) else {
        return EFAULT;
    };
    fs::abrir(ruta, flags)
}

fn sys_getdents64(fd: usize, buf: u64, len: u64) -> i64 {
    if len == 0 {
        return 0;
    }
    if !validar_rango_usuario(buf, len) {
        return EFAULT;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len as usize) };
    fs::getdents64(fd, dst)
}

fn sys_uname(buf: u64) -> i64 {
    const TAMANO_UTSNAME: u64 = 65 * 6;
    if !validar_rango_usuario(buf, TAMANO_UTSNAME) {
        return EFAULT;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, TAMANO_UTSNAME as usize) };
    dst.fill(0);

    let campos: [&[u8]; 6] = [
        b"Linux",
        b"neo_ws",
        b"6.1.0-neo_ws",
        b"#1 neo_ws Rust Framekernel",
        b"x86_64",
        b"(none)",
    ];
    for (i, texto) in campos.iter().enumerate() {
        let base = i * 65;
        let n = texto.len().min(64);
        dst[base..base + n].copy_from_slice(&texto[..n]);
    }
    0
}

fn sys_brk(nueva_dir: u64) -> u64 {
    unsafe {
        if nueva_dir == 0 || nueva_dir < BRK_INICIO {
            return BRK_ACTUAL;
        }
        if nueva_dir > BRK_INICIO + MAX_BRK_CRECIMIENTO || nueva_dir >= LIMITE_USUARIO {
            return BRK_ACTUAL;
        }
        if nueva_dir > BRK_ACTUAL {
            let pag_inicio = (BRK_ACTUAL + 4095) & !4095;
            let pag_fin = (nueva_dir + 4095) & !4095;
            let mut flags =
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
            if Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE) {
                flags |= PageTableFlags::NO_EXECUTE;
            }
            let mut dir = pag_inicio;
            while dir < pag_fin {
                let pagina = Page::<Size4KiB>::containing_address(VirtAddr::new(dir));
                if memoria::mapear_pagina_usuario_global(pagina, flags).is_err() {
                    return BRK_ACTUAL;
                }
                dir += 4096;
            }
            x86_64::instructions::tlb::flush_all();
        }
        BRK_ACTUAL = nueva_dir;
        BRK_ACTUAL
    }
}
