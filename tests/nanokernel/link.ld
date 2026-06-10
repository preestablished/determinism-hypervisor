/* Nanokernel guests (ARCH §1, §2.3): static 64-bit ELFs loaded by dh-vmm's
 * PT_LOAD loader into identity-mapped guest RAM, entered directly in long
 * mode at e_entry with RSI = &BootInfo. Loaded at 1 MiB, classic and clear
 * of the loader's low-RAM page tables / BootInfo page.
 */
ENTRY(_start)

SECTIONS
{
    . = 0x100000;

    .text : ALIGN(4096) {
        *(.text.start)        /* crt0 _start first: e_entry == 0x100000 */
        *(.text*)
    }

    .rodata : ALIGN(16) { *(.rodata*) }
    .data   : ALIGN(16) { *(.data*) }
    .bss    : ALIGN(16) { *(.bss*) *(COMMON) }

    /DISCARD/ : { *(.note*) *(.comment) *(.eh_frame*) }
}
