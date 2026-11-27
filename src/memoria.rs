// Gestion de memoria: tabla de paginas (OffsetPageTable), asignador de frames y mapeo de usuario.
use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use spin::Mutex;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

pub struct GestorMemoria {
    mapper: OffsetPageTable<'static>,
    frames: AsignadorFrames,
    offset_fisico: VirtAddr,
}

static GESTOR: Mutex<Option<GestorMemoria>> = Mutex::new(None);

/// Inicializa el mapeador de paginas usando el desplazamiento de memoria fisica del bootloader.
pub unsafe fn init(offset_fisico: VirtAddr) -> OffsetPageTable<'static> {
    let tabla_l4 = tabla_nivel_4_activa(offset_fisico);
    OffsetPageTable::new(tabla_l4, offset_fisico)
}

/// Guarda el gestor global de paginas y frames para usarlo desde `elf` y `sys_brk`.
pub fn registrar_global(
    mapper: OffsetPageTable<'static>,
    frames: AsignadorFrames,
    offset_fisico: VirtAddr,
) {
    *GESTOR.lock() = Some(GestorMemoria {
        mapper,
        frames,
        offset_fisico,
    });
}

pub fn offset_fisico() -> VirtAddr {
    GESTOR
        .lock()
        .as_ref()
        .expect("Gestor de memoria no inicializado")
        .offset_fisico
}

/// Mapea una pagina de 4 KiB accesible desde Ring 3 usando el gestor global.
pub unsafe fn mapear_pagina_usuario_global(
    pagina: Page<Size4KiB>,
    flags: PageTableFlags,
) -> Result<PhysFrame<Size4KiB>, MapToError<Size4KiB>> {
    let mut guard = GESTOR.lock();
    let g = guard.as_mut().expect("Gestor de memoria no inicializado");
    mapear_pagina_usuario(pagina, flags, &mut g.mapper, &mut g.frames, g.offset_fisico)
}

unsafe fn tabla_nivel_4_activa(offset_fisico: VirtAddr) -> &'static mut PageTable {
    let (frame_l4, _) = Cr3::read();
    let dir_fisica = frame_l4.start_address();
    let dir_virtual = offset_fisico + dir_fisica.as_u64();
    let puntero: *mut PageTable = dir_virtual.as_mut_ptr();
    &mut *puntero
}

/// Calcula cuanta RAM libre reporto el BIOS (en kilobytes).
pub fn ram_usable_kb(mapa: &MemoryMap) -> u64 {
    mapa.iter()
        .filter(|r| r.region_type == MemoryRegionType::Usable)
        .map(|r| r.range.end_addr() - r.range.start_addr())
        .sum::<u64>()
        / 1024
}

/// Mapea una pagina de 4 KiB accesible desde Ring 3 (usuario) y devuelve su frame fisico.
pub unsafe fn mapear_pagina_usuario(
    pagina: Page<Size4KiB>,
    flags: PageTableFlags,
    mapper: &mut OffsetPageTable<'static>,
    frames: &mut impl FrameAllocator<Size4KiB>,
    offset_fisico: VirtAddr,
) -> Result<PhysFrame<Size4KiB>, MapToError<Size4KiB>> {
    let flags_usuario = flags | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    let frame = if let Ok(existente) = mapper.translate_page(pagina) {
        if flags_usuario.contains(PageTableFlags::WRITABLE) {
            let _ = mapper.update_flags(pagina, flags_usuario);
        }
        existente
    } else {
        let nuevo = frames
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        // Limpiamos los 4096 bytes del frame fisico para no filtrar datos del kernel a usuario.
        let ptr = (offset_fisico + nuevo.start_address().as_u64()).as_mut_ptr::<u8>();
        core::ptr::write_bytes(ptr, 0, 4096);

        let flags_padre =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        mapper
            .map_to_with_table_flags(pagina, nuevo, flags_usuario, flags_padre, frames)?
            .flush();
        nuevo
    };

    // Aseguramos USER_ACCESSIBLE (y sin NO_EXECUTE) en las tablas intermedias (PML4 -> PDPT -> PD).
    // Las paginas del kernel siguen protegidas porque sus entradas hoja no tienen USER_ACCESSIBLE.
    asegurar_tablas_intermedias_usuario(pagina.start_address(), offset_fisico);
    Ok(frame)
}

unsafe fn asegurar_tablas_intermedias_usuario(dir: VirtAddr, offset_fisico: VirtAddr) {
    let indices = [dir.p4_index(), dir.p3_index(), dir.p2_index()];
    let mut tabla = tabla_nivel_4_activa(offset_fisico);

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

/// Asignador simple que entrega bloques (frames) de 4 KiB de RAM fisica libre.
pub struct AsignadorFrames {
    mapa: &'static MemoryMap,
    siguiente: usize,
}

impl AsignadorFrames {
    pub unsafe fn init(mapa: &'static MemoryMap) -> Self {
        AsignadorFrames { mapa, siguiente: 0 }
    }

    fn frames_usables(&self) -> impl Iterator<Item = PhysFrame> {
        let regiones = self
            .mapa
            .iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable);
        let rangos = regiones.map(|r| r.range.start_addr()..r.range.end_addr());
        let direcciones = rangos.flat_map(|r| r.step_by(4096));
        direcciones.map(|dir| PhysFrame::containing_address(PhysAddr::new(dir)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for AsignadorFrames {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.frames_usables().nth(self.siguiente);
        self.siguiente += 1;
        frame
    }
}
