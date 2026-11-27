// Cargador de binarios ejecutables ELF64 de Linux x86_64 con pila inicial System V (argc/argv).
use crate::memoria::{self, EspacioDirecciones};
use alloc::vec::Vec;
use x86_64::registers::model_specific::{Efer, EferFlags};
use x86_64::structures::paging::{Page, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

const ELF_MAGICO: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELF_CLASE_64: u8 = 2;
const ELF_LITTLE_ENDIAN: u8 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

const TOPE_PILA_USUARIO: u64 = 0x0000_7fff_f000_0000;
const PAGINAS_PILA_USUARIO: u64 = 16; // 64 KiB de pila por proceso (suficiente para Rust std/musl)
const LIMITE_USUARIO: u64 = 0x0000_8000_0000_0000;

#[repr(C)]
#[derive(Clone, Copy)]
struct CabeceraElf64 {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CabeceraPrograma64 {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub struct ProgramaCargado {
    pub punto_entrada: u64,
    pub pila_usuario: u64,
    pub fin_datos: u64,
    pub segmentos_cargados: usize,
}

pub fn cargar(
    datos: &[u8],
    espacio: &mut EspacioDirecciones,
    argv: &[&str],
) -> Result<ProgramaCargado, &'static str> {
    if datos.len() < core::mem::size_of::<CabeceraElf64>() {
        return Err("Archivo demasiado pequeno para ser ELF");
    }

    let hdr = unsafe { core::ptr::read_unaligned(datos.as_ptr() as *const CabeceraElf64) };
    if hdr.e_ident[0..4] != ELF_MAGICO {
        return Err("No tiene firma magica ELF");
    }
    if hdr.e_ident[4] != ELF_CLASE_64 || hdr.e_ident[5] != ELF_LITTLE_ENDIAN {
        return Err("Solo se soportan ELF de 64 bits little-endian");
    }
    if hdr.e_type != ET_EXEC || hdr.e_machine != EM_X86_64 {
        return Err("Solo se soportan ejecutables estaticos x86_64");
    }

    let nxe_activo = Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE);
    let offset_fisico = memoria::offset_fisico();
    let mut segmentos_cargados = 0;
    let mut max_vaddr: u64 = 0x40_0000;
    let mut phdr_vaddr: u64 = 0;

    for i in 0..hdr.e_phnum as usize {
        let pos = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        if pos + core::mem::size_of::<CabeceraPrograma64>() > datos.len() {
            return Err("Cabecera de programa fuera de rango");
        }
        let ph = unsafe {
            core::ptr::read_unaligned(datos.as_ptr().add(pos) as *const CabeceraPrograma64)
        };

        if ph.p_type == 6 {
            // PT_PHDR
            phdr_vaddr = ph.p_vaddr;
        }
        if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
            continue;
        }
        if phdr_vaddr == 0 && ph.p_offset == 0 {
            phdr_vaddr = ph.p_vaddr + hdr.e_phoff;
        }
        if ph.p_vaddr < 0x1000 || ph.p_vaddr.saturating_add(ph.p_memsz) >= LIMITE_USUARIO {
            return Err("Segmento ELF fuera del espacio de usuario");
        }
        if ph.p_offset.saturating_add(ph.p_filesz) > datos.len() as u64 {
            return Err("Datos de segmento fuera del archivo ELF");
        }

        let fin_seg = ph.p_vaddr + ph.p_memsz;
        if fin_seg > max_vaddr {
            max_vaddr = fin_seg;
        }

        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if (ph.p_flags & PF_W) != 0 {
            flags |= PageTableFlags::WRITABLE;
        }
        if nxe_activo && (ph.p_flags & PF_X) == 0 {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        let primera = Page::<Size4KiB>::containing_address(VirtAddr::new(ph.p_vaddr));
        let ultima = Page::<Size4KiB>::containing_address(VirtAddr::new(fin_seg - 1));

        for pagina in Page::range_inclusive(primera, ultima) {
            let frame = espacio.mapear_pagina(pagina, flags)?;

            let pag_inicio = pagina.start_address().as_u64();
            let pag_fin = pag_inicio + 4096;
            let datos_fin = ph.p_vaddr + ph.p_filesz;

            let copia_inicio = pag_inicio.max(ph.p_vaddr);
            let copia_fin = pag_fin.min(datos_fin);

            if copia_inicio < copia_fin {
                let cant = (copia_fin - copia_inicio) as usize;
                let src_off = (ph.p_offset + (copia_inicio - ph.p_vaddr)) as usize;
                let dst_off = (copia_inicio - pag_inicio) as usize;
                unsafe {
                    let dst = (offset_fisico + frame.start_address().as_u64())
                        .as_mut_ptr::<u8>()
                        .add(dst_off);
                    core::ptr::copy_nonoverlapping(datos.as_ptr().add(src_off), dst, cant);
                }
            }
        }
        segmentos_cargados += 1;
    }

    // Mapeamos la pila de usuario (16 KiB) y guardamos el frame superior para colocar argc/argv.
    let mut flags_pila =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    if nxe_activo {
        flags_pila |= PageTableFlags::NO_EXECUTE;
    }
    let inicio_pila = TOPE_PILA_USUARIO - PAGINAS_PILA_USUARIO * 4096;
    let mut frame_tope = None;
    for i in 0..PAGINAS_PILA_USUARIO {
        let pagina = Page::<Size4KiB>::containing_address(VirtAddr::new(inicio_pila + i * 4096));
        let frame = espacio.mapear_pagina(pagina, flags_pila)?;
        if i == PAGINAS_PILA_USUARIO - 1 {
            frame_tope = Some(frame);
        }
    }

    // Construimos la pila inicial segun el ABI de Linux x86_64:
    // [rsp] = argc, [rsp+8] = argv[0], ..., NULL, envp=NULL, auxv=(0,0)
    let frame_tope = frame_tope.ok_or("Error al mapear tope de pila")?;
    let base_tope_virt = TOPE_PILA_USUARIO - 4096;
    let base_tope_ptr = (offset_fisico + frame_tope.start_address().as_u64()).as_mut_ptr::<u8>();

    let mut sp_off: usize = 4096;

    // 16 bytes pseudoaleatorios para AT_RANDOM (usados por musl y Rust std).
    sp_off -= 16;
    let random_vaddr = base_tope_virt + sp_off as u64;
    let semilla: [u8; 16] = [
        0x4e, 0x65, 0x6f, 0x57, 0x53, 0xa7, 0x31, 0x9c,
        0x55, 0x18, 0xe2, 0x44, 0x7b, 0x09, 0xd3, 0x61,
    ];
    unsafe {
        core::ptr::copy_nonoverlapping(semilla.as_ptr(), base_tope_ptr.add(sp_off), 16);
    }

    let mut arg_vaddrs: Vec<u64> = Vec::with_capacity(argv.len());

    // Escribimos las cadenas de argv terminadas en NUL al final de la pagina superior.
    for arg in argv {
        let bytes = arg.as_bytes();
        let tam = bytes.len() + 1;
        if sp_off < tam + 384 {
            return Err("Argumentos demasiado largos para la pila");
        }
        sp_off -= tam;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), base_tope_ptr.add(sp_off), bytes.len());
            *base_tope_ptr.add(sp_off + bytes.len()) = 0;
        }
        arg_vaddrs.push(base_tope_virt + sp_off as u64);
    }

    // Vector auxiliar Linux x86_64 completo para musl libc:
    // AT_PHDR(3), AT_PHENT(4), AT_PHNUM(5), AT_PAGESZ(6), AT_ENTRY(9), AT_RANDOM(25), AT_NULL(0)
    let auxv: [(u64, u64); 7] = [
        (3, phdr_vaddr),
        (4, hdr.e_phentsize as u64),
        (5, hdr.e_phnum as u64),
        (6, 4096),
        (9, hdr.e_entry),
        (25, random_vaddr),
        (0, 0),
    ];

    // Calculamos el numero de palabras u64: argc + argv[..] + NULL + envp(NULL) + auxv
    let palabras = 1 + arg_vaddrs.len() + 1 + 1 + auxv.len() * 2;
    let bytes_tabla = palabras * 8;
    // Alineamos `user_rsp` a multiplo de 16 bytes.
    sp_off = (sp_off - bytes_tabla) & !15;

    let mut cursor = sp_off;
    let escribir_u64 = |off_en_pag: &mut usize, val: u64| unsafe {
        let p = base_tope_ptr.add(*off_en_pag) as *mut u64;
        p.write(val);
        *off_en_pag += 8;
    };

    escribir_u64(&mut cursor, arg_vaddrs.len() as u64); // argc
    for vaddr in arg_vaddrs {
        escribir_u64(&mut cursor, vaddr); // argv[i]
    }
    escribir_u64(&mut cursor, 0); // argv[argc] = NULL
    escribir_u64(&mut cursor, 0); // envp[0] = NULL
    for (k, v) in auxv {
        escribir_u64(&mut cursor, k);
        escribir_u64(&mut cursor, v);
    }

    let pila_usuario = base_tope_virt + sp_off as u64;
    let fin_datos = (max_vaddr + 4095) & !4095;

    Ok(ProgramaCargado {
        punto_entrada: hdr.e_entry,
        pila_usuario,
        fin_datos,
        segmentos_cargados,
    })
}
