// Tabla global de descriptores (GDT) y segmento de estado de tarea (TSS).
// Incluye segmentos de kernel (Ring 0) y usuario (Ring 3) en el orden que exige SYSCALL/SYSRET.
use spin::Lazy;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const PILA_DOBLE_FALLA: u16 = 0;

#[repr(C, align(16))]
struct Pila([u8; 4096 * 5]);

static mut PILA_EMERGENCIA: Pila = Pila([0; 4096 * 5]);
static mut PILA_RING0: Pila = Pila([0; 4096 * 5]);

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    // Pila del kernel cuando llega una interrupcion mientras el CPU esta en Ring 3 (usuario).
    tss.privilege_stack_table[0] = {
        let inicio = VirtAddr::from_ptr(&raw const PILA_RING0);
        inicio + core::mem::size_of::<Pila>() as u64
    };
    // Pila de emergencia exclusiva para doble falla.
    tss.interrupt_stack_table[PILA_DOBLE_FALLA as usize] = {
        let inicio = VirtAddr::from_ptr(&raw const PILA_EMERGENCIA);
        inicio + core::mem::size_of::<Pila>() as u64
    };
    tss
});

#[derive(Clone, Copy)]
pub struct Selectores {
    pub codigo_kernel: SegmentSelector,
    pub datos_kernel: SegmentSelector,
    pub datos_usuario: SegmentSelector,
    pub codigo_usuario: SegmentSelector,
    pub tss: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectores)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    // Orden obligatorio para el registro MSR STAR de SYSCALL/SYSRET:
    // 1) kernel_code, 2) kernel_data, 3) user_data, 4) user_code
    let codigo_kernel = gdt.append(Descriptor::kernel_code_segment());
    let datos_kernel = gdt.append(Descriptor::kernel_data_segment());
    let datos_usuario = gdt.append(Descriptor::user_data_segment());
    let codigo_usuario = gdt.append(Descriptor::user_code_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectores {
            codigo_kernel,
            datos_kernel,
            datos_usuario,
            codigo_usuario,
            tss,
        },
    )
});

pub fn selectores() -> Selectores {
    GDT.1
}

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS, ES, SS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.codigo_kernel);
        SS::set_reg(GDT.1.datos_kernel);
        DS::set_reg(GDT.1.datos_kernel);
        ES::set_reg(GDT.1.datos_kernel);
        load_tss(GDT.1.tss);
    }
}
