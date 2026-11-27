// Gestion de memoria: asignador de frames con liberacion y espacios de direcciones (CR3) aislados.
use alloc::vec::Vec;
use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use spin::Mutex;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

struct EstadoMemoria {
    frames: AsignadorFrames,
    offset_fisico: VirtAddr,
    pml4_kernel: PhysFrame<Size4KiB>,
    cr3_flags: Cr3Flags,
    ram_total_kb: u64,
    mmio_cursor: u64,
}

static MEMORIA: Mutex<Option<EstadoMemoria>> = Mutex::new(None);
const BASE_MMIO_KERNEL: u64 = 0x_4444_8000_0000;

/// Inicializa el mapeador temporal de arranque (usado solo antes de registrar el gestor global).
pub unsafe fn init(offset_fisico: VirtAddr) -> OffsetPageTable<'static> {
    let (frame_l4, _) = Cr3::read();
    let tabla_l4 = tabla_desde_frame(frame_l4, offset_fisico);
    OffsetPageTable::new(tabla_l4, offset_fisico)
}

/// Registra el gestor global de frames y guarda la PML4 limpia del kernel como plantilla.
pub fn registrar_global(frames: AsignadorFrames, offset_fisico: VirtAddr, ram_total_kb: u64) {
    let (pml4_kernel, cr3_flags) = Cr3::read();
    *MEMORIA.lock() = Some(EstadoMemoria {
        frames,
        offset_fisico,
        pml4_kernel,
        cr3_flags,
        ram_total_kb,
        mmio_cursor: BASE_MMIO_KERNEL,
    });
}

/// Asigna un frame fisico de 4 KiB en ceros para anillos/buffers DMA (e1000, AC97).
pub fn asignar_frame_dma() -> Option<(u64, *mut u8)> {
    let mut guard = MEMORIA.lock();
    let mem = guard.as_mut()?;
    let frame = mem.frames.allocate_frame()?;
    let phys = frame.start_address().as_u64();
    let ptr = (mem.offset_fisico + phys).as_mut_ptr::<u8>();
    unsafe {
        core::ptr::write_bytes(ptr, 0, 4096);
    }
    Some((phys, ptr))
}

/// Mapea una region MMIO PCI (como BAR0 de Intel e1000) en PML4[136] del kernel (compartida por todos los procesos).
pub fn mapear_mmio_kernel(phys: u64, tamano: usize) -> Option<u64> {
    let mut guard = MEMORIA.lock();
    let mem = guard.as_mut()?;
    let off = mem.offset_fisico;
    let phys_alin = phys & !0xFFF;
    let despl = phys - phys_alin;
    let num_paginas = ((despl as usize) + tamano + 4095) / 4096;
    let virt_base = mem.mmio_cursor;
    mem.mmio_cursor += (num_paginas as u64 + 1) * 4096;

    let mut mapper = unsafe { OffsetPageTable::new(tabla_desde_frame(mem.pml4_kernel, off), off) };
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;

    for i in 0..num_paginas {
        let p_addr = PhysAddr::new(phys_alin + (i as u64) * 4096);
        let v_addr = VirtAddr::new(virt_base + (i as u64) * 4096);
        let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(p_addr);
        let page: Page<Size4KiB> = Page::containing_address(v_addr);
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut mem.frames)
                .ok()?
                .flush();
        }
    }
    Some(virt_base + despl)
}

pub fn ram_total_mb() -> u64 {
    MEMORIA
        .lock()
        .as_ref()
        .map(|m| m.ram_total_kb / 1024)
        .unwrap_or(0)
}

pub fn offset_fisico() -> VirtAddr {
    MEMORIA
        .lock()
        .as_ref()
        .expect("Memoria no inicializada")
        .offset_fisico
}

/// Activa la tabla de paginas base del kernel (usado al terminar el ultimo proceso).
pub fn activar_kernel_cr3() {
    let (pml4, flags) = {
        let g = MEMORIA.lock();
        let m = g.as_ref().expect("Memoria no inicializada");
        (m.pml4_kernel, m.cr3_flags)
    };
    unsafe {
        Cr3::write(pml4, flags);
    }
}

unsafe fn tabla_desde_frame(
    frame: PhysFrame<Size4KiB>,
    offset_fisico: VirtAddr,
) -> &'static mut PageTable {
    let ptr = (offset_fisico + frame.start_address().as_u64()).as_mut_ptr::<PageTable>();
    &mut *ptr
}

/// Calcula cuanta RAM libre reporto el BIOS (en kilobytes).
pub fn ram_usable_kb(mapa: &MemoryMap) -> u64 {
    mapa.iter()
        .filter(|r| r.region_type == MemoryRegionType::Usable)
        .map(|r| r.range.end_addr() - r.range.start_addr())
        .sum::<u64>()
        / 1024
}

/// Asignador O(1) de frames fisicos de 4 KiB con lista de frames reciclados.
pub struct AsignadorFrames {
    mapa: &'static MemoryMap,
    reg_idx: usize,
    dir_actual: u64,
    libres: Vec<PhysFrame<Size4KiB>>,
}

impl AsignadorFrames {
    pub unsafe fn init(mapa: &'static MemoryMap) -> Self {
        AsignadorFrames {
            mapa,
            reg_idx: 0,
            dir_actual: 0,
            libres: Vec::new(),
        }
    }

    pub fn liberar_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        self.libres.push(frame);
    }
}

unsafe impl FrameAllocator<Size4KiB> for AsignadorFrames {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        if let Some(reciclado) = self.libres.pop() {
            return Some(reciclado);
        }
        while self.reg_idx < self.mapa.len() {
            let r = self.mapa[self.reg_idx];
            if r.region_type != MemoryRegionType::Usable {
                self.reg_idx += 1;
                self.dir_actual = 0;
                continue;
            }
            let inicio = (r.range.start_addr() + 4095) & !4095;
            if self.dir_actual < inicio {
                self.dir_actual = inicio;
            }
            if self.dir_actual + 4096 <= r.range.end_addr() {
                let dir = self.dir_actual;
                self.dir_actual += 4096;
                return Some(PhysFrame::containing_address(PhysAddr::new(dir)));
            }
            self.reg_idx += 1;
            self.dir_actual = 0;
        }
        None
    }
}

/// Envoltorio que registra cada frame asignado a un proceso para poder liberarlo al morir.
struct RastreadorFrames<'a> {
    base: &'a mut AsignadorFrames,
    propios: &'a mut Vec<PhysFrame<Size4KiB>>,
}

unsafe impl<'a> FrameAllocator<Size4KiB> for RastreadorFrames<'a> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let f = self.base.allocate_frame()?;
        self.propios.push(f);
        Some(f)
    }
}

/// Espacio de direcciones virtual aislado (propio CR3) para un proceso de usuario.
pub struct EspacioDirecciones {
    pml4: PhysFrame<Size4KiB>,
    paginas_usuario: Vec<(Page<Size4KiB>, PageTableFlags)>,
    frames_propios: Vec<PhysFrame<Size4KiB>>,
}

impl EspacioDirecciones {
    /// Crea un nuevo espacio de direcciones que comparte el kernel pero aisla todo el espacio de usuario.
    pub fn nuevo_vacio() -> Result<Self, &'static str> {
        let mut guard = MEMORIA.lock();
        let mem = guard.as_mut().expect("Memoria no inicializada");
        let off = mem.offset_fisico;

        let mut propios = Vec::new();
        let pml4 = mem.frames.allocate_frame().ok_or("Sin RAM para PML4")?;
        propios.push(pml4);
        let pdpt0 = mem.frames.allocate_frame().ok_or("Sin RAM para PDPT0")?;
        propios.push(pdpt0);
        let pd0 = mem.frames.allocate_frame().ok_or("Sin RAM para PD0")?;
        propios.push(pd0);

        unsafe {
            // 1) Copiamos las tablas con copy_nonoverlapping para no poner PageTables de 4 KiB en la pila.
            let k_pml4 = tabla_desde_frame(mem.pml4_kernel, off);
            let n_pml4 = tabla_desde_frame(pml4, off);
            core::ptr::copy_nonoverlapping(k_pml4 as *const PageTable, n_pml4 as *mut PageTable, 1);

            // 2) Clonamos PDPT[0] y PD[0] para que las paginas de usuario en 0x400000+ sean privadas
            //    mientras VGA (PD[0]) y el kernel (PD[1]) siguen compartidos y protegidos en Ring 0.
            let k_pdpt0_frame = PhysFrame::containing_address(k_pml4[0].addr());
            let k_pdpt0 = tabla_desde_frame(k_pdpt0_frame, off);
            let n_pdpt0 = tabla_desde_frame(pdpt0, off);
            core::ptr::copy_nonoverlapping(
                k_pdpt0 as *const PageTable,
                n_pdpt0 as *mut PageTable,
                1,
            );

            let k_pd0_frame = PhysFrame::containing_address(k_pdpt0[0].addr());
            let k_pd0 = tabla_desde_frame(k_pd0_frame, off);
            let n_pd0 = tabla_desde_frame(pd0, off);
            core::ptr::copy_nonoverlapping(k_pd0 as *const PageTable, n_pd0 as *mut PageTable, 1);

            let flags_dir =
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
            n_pdpt0[0].set_frame(pd0, flags_dir);
            n_pml4[0].set_frame(pdpt0, flags_dir);
        }

        Ok(EspacioDirecciones {
            pml4,
            paginas_usuario: Vec::new(),
            frames_propios: propios,
        })
    }

    /// Mapea una pagina de 4 KiB en este espacio de direcciones y devuelve su frame fisico.
    pub fn mapear_pagina(
        &mut self,
        pagina: Page<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<PhysFrame<Size4KiB>, &'static str> {
        let mut guard = MEMORIA.lock();
        let mem = guard.as_mut().expect("Memoria no inicializada");
        let off = mem.offset_fisico;
        let flags_usuario = flags | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

        let mut mapper = unsafe { OffsetPageTable::new(tabla_desde_frame(self.pml4, off), off) };

        let frame = if let Ok(existente) = mapper.translate_page(pagina) {
            if let Some(e) = self.paginas_usuario.iter_mut().find(|(p, _)| *p == pagina) {
                let mut combinadas = e.1;
                if flags_usuario.contains(PageTableFlags::WRITABLE) {
                    combinadas.insert(PageTableFlags::WRITABLE);
                }
                if !flags_usuario.contains(PageTableFlags::NO_EXECUTE) {
                    combinadas.remove(PageTableFlags::NO_EXECUTE);
                }
                e.1 = combinadas;
                let _ = unsafe { mapper.update_flags(pagina, combinadas) };
            }
            existente
        } else {
            let nuevo = mem
                .frames
                .allocate_frame()
                .ok_or("Sin RAM fisica para pagina de usuario")?;
            self.frames_propios.push(nuevo);
            unsafe {
                let ptr = (off + nuevo.start_address().as_u64()).as_mut_ptr::<u8>();
                core::ptr::write_bytes(ptr, 0, 4096);
            }

            let flags_padre = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;
            let mut rastreador = RastreadorFrames {
                base: &mut mem.frames,
                propios: &mut self.frames_propios,
            };
            unsafe {
                mapper
                    .map_to_with_table_flags(
                        pagina,
                        nuevo,
                        flags_usuario,
                        flags_padre,
                        &mut rastreador,
                    )
                    .map_err(|_| "Error al mapear pagina en tabla del proceso")?
                    .ignore();
            }
            self.paginas_usuario.push((pagina, flags_usuario));
            nuevo
        };

        unsafe {
            asegurar_tablas_intermedias(self.pml4, pagina.start_address(), off);
        }
        Ok(frame)
    }

    /// Mapea una pagina de memoria MMIO fisica (como el framebuffer BAR0 de la GPU) en Ring 3.
    /// No agrega el frame MMIO a `frames_propios` ni a `paginas_usuario`, asi nunca se libera ni se clona en fork.
    pub fn mapear_mmio(
        &mut self,
        pagina: Page<Size4KiB>,
        frame_fisico: PhysFrame<Size4KiB>,
        flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        let mut guard = MEMORIA.lock();
        let mem = guard.as_mut().expect("Memoria no inicializada");
        let off = mem.offset_fisico;
        let flags_usuario = flags | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

        let mut mapper = unsafe { OffsetPageTable::new(tabla_desde_frame(self.pml4, off), off) };

        if mapper.translate_page(pagina).is_err() {
            let flags_padre = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE;
            let mut rastreador = RastreadorFrames {
                base: &mut mem.frames,
                propios: &mut self.frames_propios,
            };
            unsafe {
                mapper
                    .map_to_with_table_flags(
                        pagina,
                        frame_fisico,
                        flags_usuario,
                        flags_padre,
                        &mut rastreador,
                    )
                    .map_err(|_| "Error al mapear MMIO en tabla del proceso")?
                    .ignore();
            }
        }
        unsafe {
            asegurar_tablas_intermedias(self.pml4, pagina.start_address(), off);
        }
        Ok(())
    }

    /// Traduce una pagina virtual de este espacio a su frame fisico.
    pub fn traducir(&self, pagina: Page<Size4KiB>) -> Option<PhysFrame<Size4KiB>> {
        let off = offset_fisico();
        let mapper = unsafe { OffsetPageTable::new(tabla_desde_frame(self.pml4, off), off) };
        mapper.translate_page(pagina).ok()
    }

    /// Clona byte por byte todas las paginas de usuario hacia un nuevo CR3 (para `fork()`).
    pub fn clonar(&self) -> Result<Self, &'static str> {
        let mut hijo = Self::nuevo_vacio()?;
        let off = offset_fisico();

        for &(pagina, flags) in &self.paginas_usuario {
            let Some(frame_padre) = self.traducir(pagina) else {
                continue;
            };
            let frame_hijo = hijo.mapear_pagina(pagina, flags)?;
            unsafe {
                let src = (off + frame_padre.start_address().as_u64()).as_ptr::<u8>();
                let dst = (off + frame_hijo.start_address().as_u64()).as_mut_ptr::<u8>();
                core::ptr::copy_nonoverlapping(src, dst, 4096);
            }
        }
        Ok(hijo)
    }

    /// Carga este espacio de direcciones en el registro CR3 del CPU.
    pub fn activar(&self) {
        let flags = MEMORIA
            .lock()
            .as_ref()
            .expect("Memoria no inicializada")
            .cr3_flags;
        unsafe {
            Cr3::write(self.pml4, flags);
        }
    }

    /// Devuelve todos los frames fisicos propios de este proceso al asignador global.
    /// IMPORTANTE: el CR3 activo ya debe haber cambiado a otro espacio antes de llamar esto.
    pub fn liberar(self) {
        assert!(
            Cr3::read().0 != self.pml4,
            "No se puede liberar el espacio de direcciones actualmente activo en CR3"
        );
        let mut guard = MEMORIA.lock();
        if let Some(mem) = guard.as_mut() {
            for frame in self.frames_propios {
                mem.frames.liberar_frame(frame);
            }
        }
    }
}

unsafe fn asegurar_tablas_intermedias(
    pml4_frame: PhysFrame<Size4KiB>,
    dir: VirtAddr,
    offset_fisico: VirtAddr,
) {
    let indices = [dir.p4_index(), dir.p3_index(), dir.p2_index()];
    let mut tabla = tabla_desde_frame(pml4_frame, offset_fisico);

    for idx in indices {
        let entrada = &mut tabla[idx];
        let mut f = entrada.flags();
        f.insert(PageTableFlags::USER_ACCESSIBLE);
        f.remove(PageTableFlags::NO_EXECUTE);
        entrada.set_flags(f);

        let siguiente_fis = entrada.addr();
        tabla = &mut *(offset_fisico + siguiente_fis.as_u64()).as_mut_ptr::<PageTable>();
    }
}
