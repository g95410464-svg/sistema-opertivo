// Heap del kernel: permite usar Box, Vec, String y demas tipos de `alloc`.
use linked_list_allocator::LockedHeap;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB};
use x86_64::VirtAddr;

pub const INICIO_HEAP: usize = 0x_4444_4444_0000;
pub const TAMANO_HEAP: usize = 1024 * 1024; // 1 MiB

#[global_allocator]
static ASIGNADOR: LockedHeap = LockedHeap::empty();

pub fn init(
    mapper: &mut impl Mapper<Size4KiB>,
    frames: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let inicio = VirtAddr::new(INICIO_HEAP as u64);
    let fin = inicio + TAMANO_HEAP as u64 - 1u64;
    let rango_paginas = Page::range_inclusive(
        Page::containing_address(inicio),
        Page::containing_address(fin),
    );

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

    for pagina in rango_paginas {
        let frame = frames
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        unsafe {
            mapper.map_to(pagina, frame, flags, frames)?.flush();
        }
    }

    unsafe {
        ASIGNADOR.lock().init(INICIO_HEAP as *mut u8, TAMANO_HEAP);
    }

    Ok(())
}
