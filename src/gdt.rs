// Tabla global de descriptores (GDT) y segmento de estado de tarea (TSS).
use spin::Lazy;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const PILA_DOBLE_FALLA: u16 = 0;

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[PILA_DOBLE_FALLA as usize] = {
        const TAMANO: usize = 4096 * 5;
        static mut PILA: [u8; TAMANO] = [0; TAMANO];
        let inicio = VirtAddr::from_ptr(&raw const PILA);
        inicio + TAMANO as u64
    };
    tss
});

struct Selectores {
    codigo: SegmentSelector,
    tss: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectores)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let codigo = gdt.append(Descriptor::kernel_code_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (gdt, Selectores { codigo, tss })
});

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.codigo);
        load_tss(GDT.1.tss);
    }
}