//! Fuzz DHILOG lineage splicing (bead 6zm): hostile bytes become segment
//! lists, then `Lineage::new`, `extend`, and `edges` must stay total over
//! every accepted or rejected composition.

#![no_main]

use dh_inputlog::splice::Lineage;
use dh_inputlog::{
    dhilog::{LogWriter, SealParams, SegmentHeader},
    payload_digest,
};
use libfuzzer_sys::fuzz_target;

const MAX_SEGMENTS: usize = 16;
const MAX_SEGMENT_LEN: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    exercise_generated_stitch(data);

    let raw_single = [data];
    exercise(&raw_single);

    let split = split_segments(data);
    exercise(&split);
});

fn split_segments(mut data: &[u8]) -> Vec<&[u8]> {
    let mut segments = Vec::new();
    while !data.is_empty() && segments.len() < MAX_SEGMENTS {
        let requested = if data.len() >= 2 {
            let len = u16::from_le_bytes([data[0], data[1]]) as usize;
            data = &data[2..];
            len
        } else {
            let len = data[0] as usize;
            data = &data[1..];
            len
        };
        let len = requested.min(MAX_SEGMENT_LEN).min(data.len());
        let (segment, rest) = data.split_at(len);
        segments.push(segment);
        data = rest;
    }
    segments
}

fn exercise(segments: &[&[u8]]) {
    for prefix_len in 0..=segments.len() {
        if let Ok(lineage) = Lineage::new(&segments[..prefix_len]) {
            inspect(&lineage);
        }
    }

    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    let first = [*first];
    let Ok(mut lineage) = Lineage::new(&first) else {
        return;
    };
    inspect(&lineage);

    for child in rest {
        if let Ok(next) = lineage.extend(child) {
            lineage = next;
            inspect(&lineage);
        }
    }
}

fn exercise_generated_stitch(data: &[u8]) {
    let segment_count = 2 + data.first().map_or(0, |b| (*b as usize) % 3);
    let clock = (
        1 + data.get(1).copied().unwrap_or(0) as u32,
        1 + data.get(2).copied().unwrap_or(0) as u32,
    );
    let cfg = payload_digest(data);

    let mut anchors = Vec::with_capacity(segment_count + 1);
    for index in 0..=segment_count {
        anchors.push(derived_id(data, index as u8));
    }

    let mut bytes = Vec::with_capacity(segment_count);
    for index in 0..segment_count {
        let base = anchors[index];
        let end = if index + 1 == segment_count && data.get(3).is_some_and(|b| b & 1 != 0) {
            [0u8; 32]
        } else {
            anchors[index + 1]
        };
        bytes.push(seal_segment(base, end, cfg, clock, index, data));
    }

    let segments: Vec<&[u8]> = bytes.iter().map(Vec::as_slice).collect();
    let lineage = Lineage::new(&segments).expect("generated stitch must validate");
    inspect(&lineage);

    let first = [segments[0]];
    let mut lineage = Lineage::new(&first).expect("generated root segment must validate");
    for child in &segments[1..] {
        lineage = lineage
            .extend(child)
            .expect("generated child segment must extend");
        inspect(&lineage);
    }
}

fn seal_segment(
    base: [u8; 32],
    end: [u8; 32],
    cfg: [u8; 32],
    clock: (u32, u32),
    index: usize,
    data: &[u8],
) -> Vec<u8> {
    let mut w = LogWriter::new(SegmentHeader {
        base_snapshot_id: base,
        entropy_seed: derived_id(data, 0x80 | index as u8),
        machine_config_hash: cfg,
        clock_num: clock.0,
        clock_den: clock.1,
        encoder_fingerprint: u64::from_le_bytes(
            derived_id(data, 0xC0 | index as u8)[..8]
                .try_into()
                .unwrap(),
        ),
    });
    let icount = 1_000 + index as u64;
    w.pad_set(
        icount,
        0x4000 + index as u64,
        data.get(index).copied().unwrap_or(0),
        u32::from_le_bytes(
            derived_id(data, 0x40 | index as u8)[..4]
                .try_into()
                .unwrap(),
        ),
        index as u32,
    )
    .expect("generated PAD_SET is in order and bounded");
    w.seal(SealParams {
        end_snapshot_id: end,
        end_icount: icount + 1,
        end_vns: icount + 1,
        end_state_hash: derived_id(data, 0x20 | index as u8),
        stop_reason: data.get(4 + index).copied().unwrap_or(0),
    })
    .expect("generated segment must seal")
}

fn derived_id(data: &[u8], tag: u8) -> [u8; 32] {
    let mut id = [tag; 32];
    for (index, byte) in data.iter().take(256).enumerate() {
        let lane = index % 32;
        id[lane] = id[lane].wrapping_add(*byte).rotate_left((index % 8) as u32) ^ index as u8;
    }
    id[0] |= 1;
    id
}

fn inspect(lineage: &Lineage<'_>) {
    let _ = lineage.len();
    let _ = lineage.is_empty();
    let _ = lineage.root_base();
    let _ = lineage.end_identity();

    for edge in lineage.edges() {
        let _ = edge.index;
        let _ = edge.base_snapshot_id;
        let log = edge.log;
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
    }
}
