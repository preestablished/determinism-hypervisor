//! Fuzz the entire DHILOG read path (bead 1j4): if `parse` accepts the
//! bytes, every accessor the replay/verification rails use must be
//! total — `body()` decode of every record, the canonical and aux
//! views, and the END payload (parse guarantees a sealed log carries
//! one; a panic here means parse accepted an unsealed/malformed log).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(log) = dh_inputlog::reader::LogReader::parse(data) else {
        return;
    };
    let _ = log.header();
    for rec in log.records() {
        let _ = rec.kind();
        let _ = rec.rflags();
        let _ = rec.seq();
        let _ = rec.icount();
        let _ = rec.boundary_rip();
        let _ = rec.is_aux();
        let _ = rec.body();
    }
    let _ = log.canonical().count();
    let _ = log.aux().count();
    let _ = log.end();
});
