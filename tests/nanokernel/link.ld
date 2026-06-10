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

/* Orphans (.got, .init_array, ...): pure-nasm guests produce none, and a
 * catch-all `*(*)` sink is NOT usable — lld feeds its internal synthetic
 * sections (.symtab, .strtab, .rela.plt) through it and errors on type
 * mismatches. GNU ld and lld may place real orphans differently, so the
 * elf_shape test (e_entry == 0x100000, PT_LOAD covers entry, size budget)
 * is the deliberate backstop against silent layout drift.
 */

/* Note: the whole guest intentionally lives in one RWE PT_LOAD — these are
 * ~2 KiB test guests for a hypervisor that snapshots/replays them; W^X
 * hardening buys nothing here and extra segments complicate the loader.
 * The elf_shape test asserting a PT_LOAD covers e_entry is the backstop.
 */
