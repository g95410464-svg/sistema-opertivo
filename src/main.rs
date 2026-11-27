#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod audio;
mod disco;
mod e1000;
mod elf;
mod entrada;
mod fb;
mod fs;
mod gdt;
mod heap;
mod interrupciones;
mod memoria;
mod neofs;
mod net;
mod proceso;
mod raton;
mod reloj;
mod syscall;
mod teclado;
mod vga;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use vga::Color;
use x86_64::VirtAddr;

entry_point!(kernel_main);

fn habilitar_sse() {
    use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};
    unsafe {
        Cr0::update(|f| {
            f.remove(Cr0Flags::EMULATE_COPROCESSOR);
            f.insert(Cr0Flags::MONITOR_COPROCESSOR);
        });
        Cr4::update(|f| {
            f.insert(Cr4Flags::OSFXSR | Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
}

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    vga::limpiar();
    println!("Iniciando neo_ws 2.0...");

    habilitar_sse();
    gdt::init();
    interrupciones::init();
    raton::init();
    syscall::init();

    // Inicializamos paginacion y heap del kernel (4 MiB) sobre la PML4 limpia del kernel.
    let ram_kb = memoria::ram_usable_kb(&boot_info.memory_map);
    let ram_mb = ram_kb / 1024;
    let offset_fisico = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memoria::init(offset_fisico) };
    let mut frames = unsafe { memoria::AsignadorFrames::init(&boot_info.memory_map) };

    heap::init(&mut mapper, &mut frames).expect("Fallo al inicializar el heap");
    drop(mapper);
    memoria::registrar_global(frames, offset_fisico, ram_kb);

    // Detectamos hardware grafico (PCI/BGA) y disco virtual (ATA PIO).
    let tiene_gui = fb::init().is_some();
    if let Some(f) = fb::info() {
        println!(
            "GPU Bochs BGA detectada: {}x{}x{}bpp (BAR0={:#x})",
            f.ancho, f.alto, f.bpp, f.phys_addr
        );
    }
    if let Some(d) = disco::init() {
        println!(
            "Disco ATA /dev/sda: {} ({} sectores, MBR={})",
            d.modelo,
            d.sectores,
            if d.firma_mbr_ok { "OK" } else { "NO" }
        );
    }
    if let Some(mac) = e1000::init() {
        println!(
            "Red Intel e1000 lista (MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, IP 10.0.2.15)",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );
    }
    if audio::init() {
        println!("Audio Intel AC97 PCI listo (/dev/dsp, 48 kHz estereo DMA)");
    }

    // Montamos el sistema de archivos en memoria (ramfs).
    fs::init();
    println!(
        "Memoria: {} MB RAM | Heap: {} KiB | ramfs + procesos CR3 listos",
        ram_mb,
        heap::TAMANO_HEAP / 1024
    );

    // Si hay GPU disponible, arrancamos directo en el escritorio grafico /bin/desktop.
    // Si el usuario elige "Consola texto" (codigo 42), caemos limpiamente a /bin/sh.
    let mut abrir_shell = !tiene_gui;
    if tiene_gui {
        let (entry, user_rsp, segs, brk) =
            proceso::crear_init("/bin/desktop").expect("Error al crear /bin/desktop");
        println!(
            "Iniciando entorno grafico /bin/desktop ({} segs, entry={:#x}, brk={:#x})...",
            segs, entry, brk
        );
        let codigo = syscall::ejecutar_en_usuario(entry, user_rsp);
        fb::desactivar_modo_grafico();
        if codigo == 42 {
            abrir_shell = true;
        }
    }

    if abrir_shell {
        vga::limpiar();
        let (entry, user_rsp, segs, brk) =
            proceso::crear_init("/bin/sh").expect("Error al crear proceso /bin/sh");
        println!(
            "PID 1 (/bin/sh) en CR3 aislado ({} segs, entry={:#x}, brk={:#x})\n",
            segs, entry, brk
        );
        let codigo = syscall::ejecutar_en_usuario(entry, user_rsp);
        vga::color(Color::VerdeClaro, Color::Negro);
        println!("\n[neo_ws] PID 1 (/bin/sh) termino con codigo: {}", codigo);
        vga::color(Color::Blanco, Color::Negro);
        println!("Sistema detenido de forma segura. Puedes cerrar QEMU.");
    }

    hlt_loop();
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    x86_64::instructions::interrupts::disable();
    vga::color(Color::Blanco, Color::Rojo);
    println!("{}", info);
    hlt_loop();
}
