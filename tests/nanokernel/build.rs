//! Nanokernel build pipeline (ARCH §1): assemble every top-level program in
//! asm/ (crt0.asm is the shared entry, linked into each) with nasm and link
//! a static 64-bit ELF per program into OUT_DIR.
//!
//! HOST-RUNNABLE on any dev host: nasm cross-assembles x86 everywhere, and
//! the linker is probed in portability order — the GNU ld x86_64 emulation
//! (x86 hosts), then ld.lld / lld, then the rust-lld shipped inside the
//! active Rust sysroot (always present, covers aarch64 hosts with stock
//! binutils). Execution of the built guests is HARDWARE-GATED elsewhere.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Top-level programs: one ELF each. crt0 is shared, not a program.
const PROGRAMS: &[&str] = &["pipeline_smoke"];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let asm = manifest.join("asm");
    let include = manifest.join("include");
    let link_script = manifest.join("link.ld");

    println!("cargo:rerun-if-changed={}", asm.display());
    println!("cargo:rerun-if-changed={}", include.display());
    println!("cargo:rerun-if-changed={}", link_script.display());

    let nasm = which("nasm").unwrap_or_else(|| {
        panic!(
            "nasm not found on PATH — required to build the nanokernel test \
             guests (apt-get install nasm / dnf install nasm)"
        )
    });
    let linker = find_linker();

    let crt0 = assemble(&nasm, &asm.join("crt0.asm"), &include, &out);
    for prog in PROGRAMS {
        let obj = assemble(&nasm, &asm.join(format!("{prog}.asm")), &include, &out);
        link(
            &linker,
            &link_script,
            &[&crt0, &obj],
            &out.join(format!("{prog}.elf")),
        );
    }
}

fn assemble(nasm: &Path, src: &Path, include: &Path, out: &Path) -> PathBuf {
    let obj = out.join(format!("{}.o", src.file_stem().unwrap().to_str().unwrap()));
    // -I needs the trailing separator on older nasm; harmless on new.
    let inc = format!("{}/", include.display());
    run(Command::new(nasm)
        .args(["-f", "elf64", "-I", &inc, "-o"])
        .arg(&obj)
        .arg(src));
    obj
}

/// A linker invocation recipe: the binary plus the args selecting x86_64
/// ELF output (GNU ld spells it -m elf_x86_64; lld-family is multi-target
/// via the same emulation flag).
struct Linker {
    bin: PathBuf,
    emulation: &'static [&'static str],
}

fn find_linker() -> Linker {
    let emu: &'static [&'static str] = &["-m", "elf_x86_64"];
    // GNU ld on x86 hosts (and multi-target binutils anywhere).
    if let Some(ld) = which("ld") {
        if probe(&ld, emu) {
            return Linker {
                bin: ld,
                emulation: emu,
            };
        }
    }
    for cand in ["ld.lld", "lld"] {
        if let Some(p) = which(cand) {
            if probe(&p, emu) {
                return Linker {
                    bin: p,
                    emulation: emu,
                };
            }
        }
    }
    // rust-lld ships inside every Rust sysroot: .../lib/rustlib/<host>/bin/.
    if let Ok(rustc) = std::env::var("RUSTC") {
        if let Ok(o) = Command::new(&rustc).arg("--print").arg("sysroot").output() {
            let sysroot = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if let Ok(host) = std::env::var("HOST") {
                let p = PathBuf::from(&sysroot)
                    .join("lib/rustlib")
                    .join(&host)
                    .join("bin/rust-lld");
                if p.exists() {
                    // rust-lld needs the flavor selector before ld args.
                    return Linker {
                        bin: p,
                        emulation: &["-flavor", "gnu", "-m", "elf_x86_64"],
                    };
                }
            }
        }
    }
    panic!(
        "no usable x86_64 ELF linker found: need GNU ld with the elf_x86_64 \
         emulation, ld.lld/lld on PATH, or rust-lld in the Rust sysroot"
    );
}

/// Can this linker accept the x86_64 emulation? (--version is cheap and
/// fails fast on single-target GNU ld without the emulation compiled in —
/// those reject the -m flag immediately. VERIFIED FACT, do not refactor
/// away: GNU ld validates -m BEFORE honoring --version and exits 1 on an
/// unsupported emulation, so an aarch64 single-target ld cannot
/// false-positive here.)
fn probe(bin: &Path, emu: &[&str]) -> bool {
    Command::new(bin)
        .args(emu)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn link(linker: &Linker, script: &Path, objs: &[&Path], out: &Path) {
    let mut cmd = Command::new(&linker.bin);
    cmd.args(linker.emulation)
        .arg("-nostdlib")
        .arg("--no-dynamic-linker")
        .arg("-static")
        .arg("-T")
        .arg(script)
        .arg("-o")
        .arg(out);
    for o in objs {
        cmd.arg(o);
    }
    run(&mut cmd);
}

fn run(cmd: &mut Command) {
    let rendered = format!("{cmd:?}");
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {rendered}: {e}"));
    assert!(status.success(), "command failed ({status}): {rendered}");
}

fn which(bin: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(bin)).find(|p| {
        // Executable regular file — a stray non-executable `nasm`/`ld` on
        // PATH must fall through, not turn into a spawn panic.
        p.is_file()
            && std::fs::metadata(p)
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    })
}
