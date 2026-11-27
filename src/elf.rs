// Cargador de binarios ejecutables ELF64 de Linux x86_64.
use crate::memoria;
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
const PAGINAS_PILA_USUARIO: u64 = 4; // 16 KiB de pila para el proceso
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

pub fn cargar(datos: &[u8]) -> Result<ProgramaCargado, &'static str> {
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

    for i in 0..hdr.e_phnum as usize {
        let pos = hdr.e_phoff as usize + i * hdr.e_phentsize as usize;
        if pos + core::mem::size_of::<CabeceraPrograma64>() > datos.len() {
            return Err("Cabecera de programa fuera de rango");
        }
        let ph = unsafe {
            core::ptr::read_unaligned(datos.as_ptr().add(pos) as *const CabeceraPrograma64)
        };

        if ph.p_type != PT_LOAD || ph.p_memsz == 0 {
            continue;
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
            let frame = unsafe {
                memoria::mapear_pagina_usuario_global(pagina, flags)
                    .map_err(|_| "Sin memoria para mapear segmento ELF")?
            };

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

    // Mapeamos la pila de usuario (16 KiB).
    let mut flags_pila =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    if nxe_activo {
        flags_pila |= PageTableFlags::NO_EXECUTE;
    }
    let inicio_pila = TOPE_PILA_USUARIO - PAGINAS_PILA_USUARIO * 4096;
    for i in 0..PAGINAS_PILA_USUARIO {
        let pagina = Page::<Size4KiB>::containing_address(VirtAddr::new(inicio_pila + i * 4096));
        unsafe {
            memoria::mapear_pagina_usuario_global(pagina, flags_pila)
                .map_err(|_| "Sin memoria para la pila de usuario")?;
        }
    }

    x86_64::instructions::tlb::flush_all();

    // Redondeamos el final de los segmentos ELF hacia arriba a multiplo de 4 KiB para `brk`.
    let fin_datos = (max_vaddr + 4095) & !4095;

    Ok(ProgramaCargado {
        punto_entrada: hdr.e_entry,
        pila_usuario: TOPE_PILA_USUARIO,
        fin_datos,
        segmentos_cargados,
    })
}
