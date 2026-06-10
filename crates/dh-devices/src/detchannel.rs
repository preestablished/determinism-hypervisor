//! detchannel host side (ARCH §6.6): the PIO detcall handler, doorbell /
//! pause-boundary ring drains, and DEV_EVENT logging.
//!
//! The channel itself — page layout, ring framing, event kinds, manifest
//! format, detcall register ABI — is OWNED BY guest-sdk (its ARCHITECTURE §2
//! and API.md §3–§5) and never restated here normatively. This module links
//! `detguest-host` and implements the host-side obligations:
//!
//! - every `IN` return value is a canonical DHILOG `DEV_EVENT/PIO_ANSWER`
//!   record (`DevCtx::log_pio_answer`);
//! - every host mutation of channel memory (ring pushes, consumer bumps)
//!   flows through [`detguest_host::ChannelWriteSink`] into canonical
//!   `DEV_EVENT/RING_PUSH` / `DEV_EVENT/CONS_BUMP` records;
//! - drained guest events are stamped with the exit icount and mirrored as
//!   AUX `SDK_EVENT` digests; payload forwarding to `StreamGuestEvents` is
//!   the caller's job (the drained events are returned);
//! - channel memory is touched ONLY inside a detcall exit or at a pause
//!   boundary — both entry points here take a [`DevCtx`], which the VMM
//!   constructs exactly there.
//!
//! Guest memory access: the channel reads/writes guest RAM through its own
//! [`detguest_host::GuestMem`] handle `M`, supplied at construction. In the
//! VMM, `M` is a cheap clone of the same mapping `DevCtx::mem` views; in
//! tests, both wrap one shared mock. `M: Clone` because `Channel::attach`
//! consumes the handle and a failed attach must not cost it.

use crate::ctx::DevCtx;
use detguest_host::{
    Channel, ChannelWriteSink, FaultPlan, GuestEvent, GuestMem, InjectResponder, OwnedPayload,
    RegionManifest,
};
use detguest_wire::events::{encode_event, EventPayload};
use detguest_wire::header::CHANNEL_SIZE_PAGES;
use detguest_wire::ports::{
    InitStatus, PORT_DOORBELL, PORT_IDENT, PORT_INIT_GO, PORT_INIT_HI, PORT_INIT_LO, PORT_INJECT,
    PORT_QUIESCE_ACK,
};
use detguest_wire::record::{EventKind, FLAG_REACHABLE_DECL, MAX_RECORD_LEN, RECORD_HEADER_LEN};
use detguest_wire::RingId;
use dh_inputlog::dhilog::{LogWriter, DEVICE_ID_DETCHANNEL, EVENT_CONS_BUMP, EVENT_RING_PUSH};

/// `IN 0xD370` answer (guest-sdk owns this ABI value — single source).
pub use detguest_wire::ports::IDENT_VALUE as IDENT_ANSWER;

/// `IN 0xD37C` before any INIT_GO commit. The ABI defines only the
/// post-commit statuses (0..=3); this sentinel is deliberately none of them
/// so a guest reading status before committing cannot see a stale "OK".
pub const INIT_STATUS_NEVER_COMMITTED: u32 = u32::MAX;

/// Doorbell mask bits (guest-sdk API.md §5).
pub const DOORBELL_RING_A: u32 = 1 << 0;
pub const DOORBELL_RING_W: u32 = 1 << 1;

/// Host-side metrics. Deterministic functions of guest behavior — exposed
/// for observability and test assertions, never fed back to the guest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DetChannelMetrics {
    /// OUTs to ports in the window with no defined OUT semantics (RAZ/WI).
    pub raz_wi_outs: u64,
    /// INs from ports in the window with no defined IN semantics (answer 0).
    pub raz_wi_ins: u64,
    /// `IN 0xD384` with no preceding `OUT 0xD384` this boot (answer 0).
    pub inject_in_without_out: u64,
    /// Doorbell rung with an empty ring mask (no drain performed).
    pub doorbell_empty_mask: u64,
    /// Ring drains that failed wire-level decoding (corrupt guest state).
    pub drain_failures: u64,
    /// Manifest unreadable at attach (attach itself still succeeds; the
    /// capture engine just has no region resolution until a later read).
    pub manifest_read_failures: u64,
    /// Drained events whose SDK_EVENT digest could not be computed
    /// (re-encode failure — deterministic, counted, event still forwarded).
    pub sdk_digest_failures: u64,
}

impl DetChannelMetrics {
    /// True when any counter indicates a guest-contract anomaly. Run
    /// control MUST compare metrics before/after every drain-bearing exit
    /// (mirroring the `DevCtx::log_fault` check rule) — `FAULTED` is its
    /// call, not this module's.
    pub fn any_anomaly(&self) -> bool {
        self.drain_failures > 0 || self.sdk_digest_failures > 0
    }
}

/// A host→guest push failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushChannelError {
    /// No channel attached yet (CHANNEL_INIT has not succeeded).
    NotAttached,
    /// Ring-level failure (`RingFull` retries at the next pause).
    Push(detguest_host::PushError),
}

/// The detchannel host state for one VM slot.
pub struct DetChannelHost<M: GuestMem + Clone, P: FaultPlan> {
    mem: M,
    channel: Option<Channel<M>>,
    /// Latched CHANNEL_INIT GPA halves (`OUT 0xD374` / `OUT 0xD378`).
    init_lo: u32,
    init_hi: u32,
    /// Last INIT_GO status (`IN 0xD37C`).
    init_status: u32,
    /// Channel base GPA recorded at successful attach (DHSNAP `EVTC`
    /// metadata; INTEGRATION.md restore re-attaches at this GPA).
    channel_gpa: Option<u64>,
    /// Manifest snapshot from attach (capture-engine region resolution).
    manifest: Option<RegionManifest>,
    /// `OUT 0xD384` latch: the iseq the next `IN 0xD384` answers for.
    /// Boot-scoped BY DESIGN: OUT and IN are separate VM exits, so the
    /// latch must survive across exits; it persists until consumed by an
    /// IN. A guest that INs much later still gets "the selected iseq"
    /// (guest-sdk API.md §5) — deterministically.
    inject_iseq: Option<u32>,
    responder: InjectResponder<P>,
    /// Low 32 bits of the last QUIESCE_ACK token (run control reads this).
    last_quiesce_ack: Option<u32>,
    /// Last drain failure, kept alongside the count for diagnostics.
    pub last_drain_error: Option<detguest_host::WireError>,
    pub metrics: DetChannelMetrics,
}

impl<M: GuestMem + Clone, P: FaultPlan> DetChannelHost<M, P> {
    /// `mem` is the host's guest-physical access handle (the same memory
    /// `DevCtx::mem` views); `plan` decides INJECT answers (recording: the
    /// synthesizer's plan; replay: the log-backed plan).
    pub fn new(mem: M, plan: P) -> Self {
        DetChannelHost {
            mem,
            channel: None,
            init_lo: 0,
            init_hi: 0,
            init_status: INIT_STATUS_NEVER_COMMITTED,
            channel_gpa: None,
            manifest: None,
            inject_iseq: None,
            responder: InjectResponder::new(plan),
            last_quiesce_ack: None,
            last_drain_error: None,
            metrics: DetChannelMetrics::default(),
        }
    }

    /// Channel ring C/I producer seqs — the NON-RECONSTRUCTIBLE host state
    /// (guest-sdk channel.rs: the host produces those rings and never
    /// drains them). The snapshot layer must save these and feed them back
    /// via [`Self::restore_producer_seqs`] after a restore re-attach.
    pub fn producer_seqs(&self) -> Option<detguest_host::ProducerSeqs> {
        self.channel.as_ref().map(|c| c.producer_seqs())
    }

    /// Restore checkpointed producer seqs after a restore re-attach. Pure
    /// host state — touches no channel memory, so no sink involvement.
    pub fn restore_producer_seqs(&mut self, seqs: detguest_host::ProducerSeqs) {
        if let Some(c) = self.channel.as_mut() {
            c.restore_producer_seqs(seqs);
        }
    }

    /// Attached channel base GPA (per-slot metadata, DHSNAP `EVTC`).
    pub fn channel_gpa(&self) -> Option<u64> {
        self.channel_gpa
    }

    /// DHSNAP `EVTC` section CONTENTS (framing — tag, version, length —
    /// is dh-snapshot's, same convention as `DetDevice::snapshot`):
    /// the host-only detchannel state. Layout v1, all LE:
    ///   init_lo u32 | init_hi u32 | init_status u32
    ///   | inject_iseq    u8 flag + u32
    ///   | quiesce_ack    u8 flag + u32
    ///   | channel        u8 flag + gpa u64 + seq_c u32 + seq_i u32
    ///
    /// NOT serialized, by design: the manifest snapshot (guest RAM —
    /// restore re-reads it at re-attach, INTEGRATION contract) and the
    /// channel's intern/pending-inject caches (reconstructible from the
    /// drained event stream, which the ORCHESTRATOR holds — guest-sdk
    /// channel.rs; until it replays them, post-restore inject-point name
    /// resolution degrades to None, which FaultPlans must tolerate).
    pub fn snapshot(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.init_lo.to_le_bytes());
        out.extend_from_slice(&self.init_hi.to_le_bytes());
        out.extend_from_slice(&self.init_status.to_le_bytes());
        out.push(self.inject_iseq.is_some() as u8);
        out.extend_from_slice(&self.inject_iseq.unwrap_or(0).to_le_bytes());
        out.push(self.last_quiesce_ack.is_some() as u8);
        out.extend_from_slice(&self.last_quiesce_ack.unwrap_or(0).to_le_bytes());
        match (&self.channel, self.channel_gpa) {
            (Some(c), Some(gpa)) => {
                let seqs = c.producer_seqs();
                out.push(1);
                out.extend_from_slice(&gpa.to_le_bytes());
                out.extend_from_slice(&seqs.ring_c.to_le_bytes());
                out.extend_from_slice(&seqs.ring_i.to_le_bytes());
            }
            _ => {
                out.push(0);
                out.extend_from_slice(&[0u8; 16]);
            }
        }
    }

    /// EVTC section byte length (fixed in v1).
    pub const EVTC_LEN: usize = 4 + 4 + 4 + 5 + 5 + 1 + 16;
    pub const EVTC_VERSION: u16 = 1;

    /// Restore from EVTC contents. PRECONDITION (§8.3 restore order):
    /// guest RAM is already restored — re-attach validates the live
    /// channel header at the recorded GPA, then the NON-RECONSTRUCTIBLE
    /// ring C/I producer seqs are reinstated and the manifest re-read.
    pub fn restore(&mut self, bytes: &[u8], version: u16) -> Result<(), crate::RestoreError> {
        if version != Self::EVTC_VERSION || bytes.len() != Self::EVTC_LEN {
            return Err(crate::RestoreError);
        }
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let init_lo = u32_at(0);
        let init_hi = u32_at(4);
        let init_status = u32_at(8);
        let inject_iseq = (bytes[12] == 1).then(|| u32_at(13));
        let last_quiesce_ack = (bytes[17] == 1).then(|| u32_at(18));

        let (channel, channel_gpa, manifest) = if bytes[22] == 1 {
            let gpa = u64::from_le_bytes(bytes[23..31].try_into().unwrap());
            let seqs = detguest_host::ProducerSeqs {
                ring_c: u32_at(31),
                ring_i: u32_at(35),
            };
            let mut ch = Channel::attach(self.mem.clone(), gpa).map_err(|_| crate::RestoreError)?;
            ch.restore_producer_seqs(seqs);
            let manifest = ch.read_manifest().ok();
            if manifest.is_none() {
                self.metrics.manifest_read_failures += 1;
            }
            (Some(ch), Some(gpa), manifest)
        } else {
            (None, None, None)
        };

        self.init_lo = init_lo;
        self.init_hi = init_hi;
        self.init_status = init_status;
        self.inject_iseq = inject_iseq;
        self.last_quiesce_ack = last_quiesce_ack;
        self.channel = channel;
        self.channel_gpa = channel_gpa;
        self.manifest = manifest;
        Ok(())
    }

    /// Manifest snapshot taken at attach (None until attached, or if the
    /// manifest was unreadable then — see `metrics.manifest_read_failures`).
    /// STALE after later guest region registrations: callers needing live
    /// resolution re-read via `channel().read_manifest()` (it is guest RAM;
    /// seqlock-consistent at any paused boundary).
    pub fn manifest(&self) -> Option<&RegionManifest> {
        self.manifest.as_ref()
    }

    /// Read-only channel state (intern names, drop counters, producer seqs
    /// for checkpointing). Mutation happens only through the methods below,
    /// which route every channel-memory write through the logging sink.
    pub fn channel(&self) -> Option<&Channel<M>> {
        self.channel.as_ref()
    }

    /// Push a command onto ring C (vCPU paused — pause boundary). The ring
    /// write and producer-index publish are logged as DEV_EVENT/RING_PUSH.
    /// `Push(RingFull)` is retried by the caller at the next pause, never
    /// spun on.
    pub fn push_command(
        &mut self,
        cmd: &detguest_wire::Command,
        ctx: &mut DevCtx,
    ) -> Result<(), PushChannelError> {
        let channel = self.channel.as_mut().ok_or(PushChannelError::NotAttached)?;
        let mut sink = CtxSink { ctx };
        channel
            .push_command(cmd, &mut sink)
            .map_err(PushChannelError::Push)
    }

    /// Push a workload-control record onto ring I (quiesce relay in v1).
    pub fn push_workload_ctrl(
        &mut self,
        rec: &detguest_wire::WorkloadCtrl,
        ctx: &mut DevCtx,
    ) -> Result<(), PushChannelError> {
        let channel = self.channel.as_mut().ok_or(PushChannelError::NotAttached)?;
        let mut sink = CtxSink { ctx };
        channel
            .push_workload_ctrl(rec, &mut sink)
            .map_err(PushChannelError::Push)
    }

    /// Low 32 bits of the last `OUT 0xD388` token, consumed by run control.
    pub fn take_quiesce_ack(&mut self) -> Option<u32> {
        self.last_quiesce_ack.take()
    }

    /// Handle `OUT` to a detcall port, inside the exit (vCPU paused).
    /// Returns the events drained by this OUT (DOORBELL / INJECT may drain;
    /// the caller forwards them to `StreamGuestEvents`).
    pub fn pio_out(&mut self, port: u16, value: u32, ctx: &mut DevCtx) -> Vec<GuestEvent> {
        match port {
            PORT_INIT_LO => {
                self.init_lo = value;
                Vec::new()
            }
            PORT_INIT_HI => {
                self.init_hi = value;
                Vec::new()
            }
            PORT_INIT_GO => {
                self.init_status = self.channel_init(value) as u32;
                Vec::new()
            }
            PORT_DOORBELL => {
                // Mask bit0 = ring A, bit1 = ring W. detguest-host drains
                // both guest→host rings in one call; any nonzero mask drains
                // both — a superset of the request, always legal (rings are
                // drained unconditionally at every pause boundary anyway).
                if value & (DOORBELL_RING_A | DOORBELL_RING_W) == 0 {
                    self.metrics.doorbell_empty_mask += 1;
                    return Vec::new();
                }
                self.drain(ctx)
            }
            PORT_INJECT => {
                // Sequencing rule (guest-sdk API.md §5): the InjectQuery is
                // already on ring W with a released producer index — drain
                // inside this exit so the matching query is folded before
                // the answering IN.
                self.inject_iseq = Some(value);
                self.drain(ctx)
            }
            PORT_QUIESCE_ACK => {
                self.last_quiesce_ack = Some(value);
                Vec::new()
            }
            _ => {
                // RAZ/WI: unknown ports in the window ignore writes.
                self.metrics.raz_wi_outs += 1;
                Vec::new()
            }
        }
    }

    /// Handle `IN` from a detcall port, inside the exit. Returns the answer
    /// value — the caller copies it into the kvm_run IO buffer (the
    /// classify_exit IN-FILL contract). Every answer is logged as a
    /// canonical DEV_EVENT/PIO_ANSWER record.
    pub fn pio_in(&mut self, port: u16, ctx: &mut DevCtx) -> u32 {
        let value = match port {
            PORT_IDENT => IDENT_ANSWER,
            PORT_INIT_LO => self.init_lo,
            PORT_INIT_HI => self.init_hi,
            PORT_INIT_GO => self.init_status,
            PORT_DOORBELL | PORT_QUIESCE_ACK => 0,
            PORT_INJECT => {
                // The responder reports the answer through the sink itself
                // (ChannelWriteSink::pio_answer) — return early so it is
                // not logged twice.
                return self.inject_answer(ctx);
            }
            _ => {
                self.metrics.raz_wi_ins += 1;
                0
            }
        };
        ctx.log_pio_answer(port, value);
        value
    }

    /// Pause-boundary drain: rings A and W unconditionally (ARCH §6.6).
    /// Returns the drained events for forwarding.
    pub fn drain_at_pause(&mut self, ctx: &mut DevCtx) -> Vec<GuestEvent> {
        self.drain(ctx)
    }

    /// CHANNEL_INIT commit (`OUT 0xD37C`). Validates the committed size and
    /// the latched GPA, attaches, records the GPA, and snapshots the
    /// manifest (seqlock-consistent).
    fn channel_init(&mut self, size_pages: u32) -> InitStatus {
        if self.channel.is_some() {
            return InitStatus::AlreadyAttached;
        }
        // Wrong committed size is a malformed commit — the header at the
        // GPA cannot be a valid v1 channel of that size (class 2, same as
        // guest-sdk maps ring-descriptor problems).
        if size_pages != CHANNEL_SIZE_PAGES {
            return InitStatus::BadMagicVersion;
        }
        let gpa = (u64::from(self.init_hi) << 32) | u64::from(self.init_lo);
        // 2 MiB alignment is a GPA-class failure (guest-sdk API.md §5:
        // "unmapped / misaligned / out of guest RAM" → status 1).
        if !gpa.is_multiple_of(u64::from(CHANNEL_SIZE_PAGES) * 4096) {
            return InitStatus::BadGpa;
        }
        match Channel::attach(self.mem.clone(), gpa) {
            Ok(ch) => {
                match ch.read_manifest() {
                    Ok(m) => self.manifest = Some(m),
                    Err(_) => self.metrics.manifest_read_failures += 1,
                }
                self.channel = Some(ch);
                self.channel_gpa = Some(gpa);
                InitStatus::Ok
            }
            Err(e) => e.init_status(),
        }
    }

    /// Answer `IN 0xD384` for the latched iseq. The responder logs the
    /// PIO_ANSWER through the sink (guest-sdk inject.rs owns the unmatched
    /// → Proceed-plus-metric rule).
    fn inject_answer(&mut self, ctx: &mut DevCtx) -> u32 {
        let Some(iseq) = self.inject_iseq.take() else {
            // No OUT latched an iseq this boot — answer Proceed and log it
            // here (the responder is keyed by iseq; inventing one could
            // accidentally match a real pending query).
            self.metrics.inject_in_without_out += 1;
            ctx.log_pio_answer(PORT_INJECT, 0);
            return 0;
        };
        let Some(channel) = self.channel.as_mut() else {
            // INJECT before CHANNEL_INIT: deterministic Proceed.
            self.metrics.inject_in_without_out += 1;
            ctx.log_pio_answer(PORT_INJECT, 0);
            return 0;
        };
        let mut sink = CtxSink { ctx };
        self.responder.answer(channel, iseq, &mut sink)
    }

    /// Drain rings A and W through the logging sink, stamp each event via
    /// an AUX SDK_EVENT digest record, and hand the events to the caller.
    fn drain(&mut self, ctx: &mut DevCtx) -> Vec<GuestEvent> {
        let Some(channel) = self.channel.as_mut() else {
            return Vec::new();
        };
        let events = {
            let mut sink = CtxSink { ctx };
            match channel.drain_events(&mut sink) {
                Ok(events) => events,
                Err(e) => {
                    // Wire-level corruption is a deterministic function of
                    // guest state: count it and surface nothing. Run control
                    // MUST check metrics after drain-bearing exits (see
                    // `DetChannelMetrics::any_anomaly`) — FAULTED is its
                    // call, not this module's.
                    self.metrics.drain_failures += 1;
                    self.last_drain_error = Some(e);
                    return Vec::new();
                }
            }
        };
        for ev in &events {
            match sdk_event_digest(ev) {
                Some((stream, len, digest8)) => ctx.log_sdk_event(stream, len, digest8),
                None => self.metrics.sdk_digest_failures += 1,
            }
        }
        events
    }
}

/// Bridges [`ChannelWriteSink`] onto the DevCtx log wrappers: every host
/// mutation of channel memory becomes a canonical DEV_EVENT at this exit's
/// icount (API.md §3.3 encodings).
struct CtxSink<'c, 'a> {
    ctx: &'c mut DevCtx<'a>,
}

/// API.md §3.3 ring ids: 0 = C (host→guest), 1 = I (host→guest),
/// 2 = A (guest→host), 3 = W (guest→host).
fn ring_id_byte(ring: RingId) -> u8 {
    match ring {
        RingId::C => 0,
        RingId::I => 1,
        RingId::A => 2,
        RingId::W => 3,
    }
}

impl ChannelWriteSink for CtxSink<'_, '_> {
    fn ring_push(&mut self, ring: RingId, bytes: &[u8], new_prod: u32) {
        // RING_PUSH: ring_id u8, _pad [u8;3], new_prod u32 LE, record bytes.
        let mut data = Vec::with_capacity(8 + bytes.len());
        data.push(ring_id_byte(ring));
        data.extend_from_slice(&[0u8; 3]);
        data.extend_from_slice(&new_prod.to_le_bytes());
        data.extend_from_slice(bytes);
        self.ctx
            .log_dev_event(DEVICE_ID_DETCHANNEL, EVENT_RING_PUSH, &data);
    }

    fn cons_bump(&mut self, ring: RingId, new_cons: u32) {
        // CONS_BUMP: ring_id u8, _pad [u8;3], new_cons u32 LE.
        let mut data = [0u8; 8];
        data[0] = ring_id_byte(ring);
        data[4..8].copy_from_slice(&new_cons.to_le_bytes());
        self.ctx
            .log_dev_event(DEVICE_ID_DETCHANNEL, EVENT_CONS_BUMP, &data);
    }

    fn pio_answer(&mut self, port: u16, value: u32) {
        // Sole PIO_ANSWER emitter for the INJECT path (the responder calls
        // here); IDENT/INIT/RAZ answers log via `DevCtx::log_pio_answer`
        // directly in `pio_in`. Exactly one record per IN, never doubled.
        self.ctx.log_pio_answer(port, value);
    }
}

/// The wire (kind, payload) pair for a drained event — one match so the
/// kind and the re-borrowed payload cannot drift apart structurally.
fn wire_view(p: &OwnedPayload) -> Option<(EventKind, EventPayload<'_>)> {
    Some(match p {
        OwnedPayload::Hello {
            proto_version,
            agent_version,
            capabilities,
        } => (
            EventKind::Hello,
            EventPayload::Hello {
                proto_version: *proto_version,
                agent_version: *agent_version,
                capabilities: *capabilities,
            },
        ),
        OwnedPayload::NameIntern { name_id, name, .. } => (
            EventKind::NameIntern,
            EventPayload::NameIntern {
                name_id: *name_id,
                name,
            },
        ),
        OwnedPayload::AssertViolation {
            name_id,
            violation_count,
            details,
        } => (
            EventKind::AssertViolation,
            EventPayload::AssertViolation {
                name_id: *name_id,
                violation_count: *violation_count,
                details,
            },
        ),
        OwnedPayload::Reachable { name_id } => (
            EventKind::Reachable,
            EventPayload::Reachable { name_id: *name_id },
        ),
        OwnedPayload::Beacon { beacon_id } => (
            EventKind::Beacon,
            EventPayload::Beacon {
                beacon_id: *beacon_id,
            },
        ),
        OwnedPayload::InjectQuery { iseq, name_id } => (
            EventKind::InjectQuery,
            EventPayload::InjectQuery {
                iseq: *iseq,
                name_id: *name_id,
            },
        ),
        OwnedPayload::RegionRegister(r) => {
            (EventKind::RegionRegister, EventPayload::RegionRegister(*r))
        }
        OwnedPayload::RegionUpdate(r) => (EventKind::RegionUpdate, EventPayload::RegionUpdate(*r)),
        OwnedPayload::WorkloadStarted { guest_pid, unit } => (
            EventKind::WorkloadStarted,
            EventPayload::WorkloadStarted {
                guest_pid: *guest_pid,
                unit: *unit,
            },
        ),
        OwnedPayload::WorkloadExited {
            guest_pid,
            exit_code,
            term_signal,
        } => (
            EventKind::WorkloadExited,
            EventPayload::WorkloadExited {
                guest_pid: *guest_pid,
                exit_code: *exit_code,
                term_signal: *term_signal,
            },
        ),
        OwnedPayload::LogLine { stream, level, msg } => (
            EventKind::LogLine,
            EventPayload::LogLine {
                stream: *stream,
                level: *level,
                msg,
            },
        ),
        OwnedPayload::QuiesceReady { token } => (
            EventKind::QuiesceReady,
            EventPayload::QuiesceReady { token: *token },
        ),
        OwnedPayload::FrameMark { frame_index } => (
            EventKind::FrameMark,
            EventPayload::FrameMark {
                frame_index: *frame_index,
            },
        ),
        OwnedPayload::Ready {
            unit,
            region_count,
            manifest_generation,
        } => (
            EventKind::Ready,
            EventPayload::Ready {
                unit: *unit,
                region_count: *region_count,
                manifest_generation: *manifest_generation,
            },
        ),
        // OwnedPayload is non_exhaustive: a future guest-sdk variant this
        // build does not know cannot be digested — counted, not dropped.
        _ => return None,
    })
}

/// SDK_EVENT fields for a drained event: (stream = EventKind, payload len,
/// digest8 over the payload bytes).
///
/// The drain decodes the record, so the original ring bytes are gone; the
/// digest input is the CANONICAL RE-ENCODING of the payload through
/// detguest-wire's own encoder — byte-identical to what the guest produced
/// for every in-cap field, and (load-bearing) computed by this same code on
/// both record and replay, so verification's digest comparison holds.
/// ASSUMPTION: record and replay run the SAME build of detguest-wire (the
/// sibling path dep is HEAD-wins); a verifier replaying an old log with a
/// changed encoder can see spurious SDK-digest divergence — encoder-skew
/// fingerprinting is tracked in beads.
fn sdk_event_digest(ev: &GuestEvent) -> Option<(u16, u32, u64)> {
    let (kind, payload) = wire_view(&ev.payload)?;
    let extra_flags = match &ev.payload {
        OwnedPayload::NameIntern {
            reachable_decl: true,
            ..
        } => FLAG_REACHABLE_DECL,
        _ => 0,
    };
    let mut buf = vec![0u8; MAX_RECORD_LEN];
    let n = encode_event(&mut buf, ev.seq, ev.vnanos, extra_flags, &payload).ok()?;
    // `len` and the digest cover the record's payload section as framed on
    // the wire — i.e. INCLUDING its 8-byte-alignment zero padding — by
    // design (one definition: "the bytes after the record header").
    let payload_bytes = &buf[RECORD_HEADER_LEN..n];
    Some((
        kind as u16,
        payload_bytes.len() as u32,
        LogWriter::digest8(payload_bytes),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::VecGuestMem;
    use detguest_host::{LogFaultPlan, MemError, MockGuestMem, TableFaultPlan};
    use detguest_wire::events::QuiesceMode;
    use detguest_wire::header::OFF_MANIFEST;
    use detguest_wire::header::{ChannelHeader, CHANNEL_SIZE, OFF_RESERVED};
    use detguest_wire::manifest::{init_manifest, MANIFEST_TOTAL_SIZE};
    use detguest_wire::{Command, FaultDecision};
    use dh_inputlog::dhilog::{SegmentHeader, KIND_DEV_EVENT, KIND_SDK_EVENT};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 2 MiB-aligned channel base for tests.
    const BASE: u64 = 0x1000_0000;

    /// Both the channel's `M` and assertions share one mock guest RAM.
    #[derive(Clone)]
    struct SharedMem(Rc<RefCell<MockGuestMem>>);

    impl GuestMem for SharedMem {
        fn read(&self, gpa: u64, buf: &mut [u8]) -> Result<(), MemError> {
            self.0.borrow().read(gpa, buf)
        }
        fn write(&mut self, gpa: u64, buf: &[u8]) -> Result<(), MemError> {
            self.0.borrow_mut().write(gpa, buf)
        }
    }

    fn channel_page() -> SharedMem {
        let mut gm = MockGuestMem::with_zeroed(BASE, CHANNEL_SIZE);
        let mut hdr = [0u8; OFF_RESERVED];
        ChannelHeader::canonical().write_to(&mut hdr).unwrap();
        gm.write(BASE, &hdr).unwrap();
        let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
        init_manifest(&mut area).unwrap();
        gm.write(BASE + OFF_MANIFEST as u64, &area).unwrap();
        SharedMem(Rc::new(RefCell::new(gm)))
    }

    fn log() -> LogWriter {
        LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
        })
    }

    /// Run `f` with a DevCtx at the given icount over the given log.
    fn with_ctx<R>(l: &mut LogWriter, icount: u64, f: impl FnOnce(&mut DevCtx) -> R) -> R {
        let mut mem = VecGuestMem(vec![0u8; 16]);
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(icount, 0xFFF0, l, &mut mem, &mut ent, &mut q);
        let r = f(&mut ctx);
        assert_eq!(ctx.log_fault(), None, "log fault during dispatch");
        r
    }

    /// Attach via the PIO protocol; returns the host with status asserted OK.
    fn attached(mem: SharedMem, l: &mut LogWriter) -> DetChannelHost<SharedMem, LogFaultPlan> {
        let mut host = DetChannelHost::new(mem, LogFaultPlan::default());
        with_ctx(l, 10, |ctx| {
            host.pio_out(PORT_INIT_LO, BASE as u32, ctx);
            host.pio_out(PORT_INIT_HI, (BASE >> 32) as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), InitStatus::Ok as u32);
        });
        host
    }

    /// Write `events` contiguously into `ring` and publish the producer.
    fn put_ring(mem: &SharedMem, ring: RingId, events: &[EventPayload<'_>]) {
        let desc = ring.canonical_desc();
        let mut off = 0u32;
        for (seq, ev) in events.iter().enumerate() {
            let mut buf = [0u8; 4096];
            let n = encode_event(&mut buf, seq as u32, 1, 0, ev).unwrap();
            mem.0
                .borrow_mut()
                .write(BASE + desc.offset as u64 + off as u64, &buf[..n])
                .unwrap();
            off += n as u32;
        }
        mem.0
            .borrow_mut()
            .write(BASE + ring.prod_offset() as u64, &off.to_le_bytes())
            .unwrap();
    }

    fn put_ring_w(mem: &SharedMem, events: &[EventPayload<'_>]) {
        put_ring(mem, RingId::W, events);
    }

    /// Decode a sealed DHILOG into (kind, payload) pairs.
    fn records(sealed: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut out = Vec::new();
        let mut at = 256;
        while at + 24 <= sealed.len() {
            let kind = sealed[at];
            let plen = u16::from_le_bytes(sealed[at + 2..at + 4].try_into().unwrap()) as usize;
            let payload = sealed[at + 24..at + 24 + plen].to_vec();
            out.push((kind, payload));
            at += 24 + plen;
            at += (8 - at % 8) % 8;
        }
        out
    }

    fn seal(l: LogWriter) -> Vec<u8> {
        l.seal(dh_inputlog::dhilog::SealParams {
            end_snapshot_id: [0; 32],
            end_icount: 1_000_000,
            end_vns: 0,
            end_state_hash: [0; 32],
            stop_reason: 1,
        })
        .unwrap()
    }

    #[test]
    fn ident_answers_magic_and_logs() {
        let mut l = log();
        let mut host = DetChannelHost::new(channel_page(), LogFaultPlan::default());
        let before = l.record_count();
        let v = with_ctx(&mut l, 5, |ctx| host.pio_in(PORT_IDENT, ctx));
        assert_eq!(v, IDENT_ANSWER);
        assert_eq!(l.record_count(), before + 1, "IN must log PIO_ANSWER");
    }

    #[test]
    fn unknown_ports_are_raz_wi_and_ins_still_log() {
        let mut l = log();
        let mut host = DetChannelHost::new(channel_page(), LogFaultPlan::default());
        with_ctx(&mut l, 5, |ctx| {
            host.pio_out(0xD390, 0xDEAD_BEEF, ctx);
            assert_eq!(host.pio_in(0xD390, ctx), 0);
        });
        assert_eq!(host.metrics.raz_wi_outs, 1);
        assert_eq!(host.metrics.raz_wi_ins, 1);
        assert_eq!(l.record_count(), 1); // the RAZ IN answer is still canonical
    }

    #[test]
    fn channel_init_attaches_and_reads_manifest() {
        let mut l = log();
        let host = attached(channel_page(), &mut l);
        assert_eq!(host.channel_gpa(), Some(BASE));
        assert!(host.manifest().is_some());
        assert_eq!(host.metrics.manifest_read_failures, 0);
    }

    #[test]
    fn init_status_codes() {
        let mut l = log();
        // Before any commit: sentinel, not a valid status.
        let mut host = DetChannelHost::new(channel_page(), LogFaultPlan::default());
        with_ctx(&mut l, 1, |ctx| {
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), INIT_STATUS_NEVER_COMMITTED);
        });

        // Wrong committed size → 2 (malformed commit).
        with_ctx(&mut l, 2, |ctx| {
            host.pio_out(PORT_INIT_LO, BASE as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES - 1, ctx);
            assert_eq!(
                host.pio_in(PORT_INIT_GO, ctx),
                InitStatus::BadMagicVersion as u32
            );
        });

        // Misaligned GPA → 1.
        with_ctx(&mut l, 3, |ctx| {
            host.pio_out(PORT_INIT_LO, (BASE + 0x1000) as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), InitStatus::BadGpa as u32);
        });

        // Unmapped (but aligned) GPA → 1.
        with_ctx(&mut l, 4, |ctx| {
            host.pio_out(PORT_INIT_LO, 0, ctx);
            host.pio_out(PORT_INIT_HI, 0x40, ctx); // 0x40_0000_0000: unmapped
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), InitStatus::BadGpa as u32);
        });

        // Valid commit → 0; a second commit → 3.
        with_ctx(&mut l, 5, |ctx| {
            host.pio_out(PORT_INIT_LO, BASE as u32, ctx);
            host.pio_out(PORT_INIT_HI, (BASE >> 32) as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), InitStatus::Ok as u32);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(
                host.pio_in(PORT_INIT_GO, ctx),
                InitStatus::AlreadyAttached as u32
            );
        });
    }

    #[test]
    fn zeroed_page_fails_magic() {
        let mut l = log();
        let gm = MockGuestMem::with_zeroed(BASE, CHANNEL_SIZE);
        let mem = SharedMem(Rc::new(RefCell::new(gm)));
        let mut host = DetChannelHost::new(mem, LogFaultPlan::default());
        with_ctx(&mut l, 1, |ctx| {
            host.pio_out(PORT_INIT_LO, BASE as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(
                host.pio_in(PORT_INIT_GO, ctx),
                InitStatus::BadMagicVersion as u32
            );
        });
    }

    #[test]
    fn doorbell_drains_logs_cons_bump_and_sdk_digests() {
        let mut l = log();
        let mem = channel_page();
        let mut host = attached(mem.clone(), &mut l);
        put_ring_w(
            &mem,
            &[
                EventPayload::NameIntern {
                    name_id: 1,
                    name: b"io_read",
                },
                EventPayload::FrameMark { frame_index: 7 },
            ],
        );

        // Empty mask: no drain.
        let none = with_ctx(&mut l, 20, |ctx| host.pio_out(PORT_DOORBELL, 0, ctx));
        assert!(none.is_empty());
        assert_eq!(host.metrics.doorbell_empty_mask, 1);

        let events = with_ctx(&mut l, 21, |ctx| {
            let events = host.pio_out(PORT_DOORBELL, DOORBELL_RING_W, ctx);
            assert_eq!(host.pio_in(PORT_DOORBELL, ctx), 0);
            events
        });
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1].payload,
            OwnedPayload::FrameMark { frame_index: 7 }
        ));

        let recs = records(&seal(l));
        // One CONS_BUMP (ring W, drained both events in one bump), two
        // SDK_EVENT digests, plus the doorbell IN answer.
        let dev_events: Vec<_> = recs.iter().filter(|(k, _)| *k == KIND_DEV_EVENT).collect();
        let cons: Vec<_> = dev_events
            .iter()
            .filter(|(_, p)| u16::from_le_bytes(p[2..4].try_into().unwrap()) == EVENT_CONS_BUMP)
            .collect();
        assert_eq!(cons.len(), 1);
        // DEV_EVENT payload: device_id u16, event_type u16, data_len u32,
        // then the data — whose first byte is the ring id (W = 3).
        assert_eq!(cons[0].1[8], 3, "ring id byte must be W=3");
        let sdk = recs.iter().filter(|(k, _)| *k == KIND_SDK_EVENT).count();
        assert_eq!(sdk, 2);
        // stream field of the second SDK_EVENT = FrameMark kind (13).
        let sdk_payloads: Vec<_> = recs
            .iter()
            .filter(|(k, _)| *k == KIND_SDK_EVENT)
            .map(|(_, p)| p.clone())
            .collect();
        assert_eq!(
            u16::from_le_bytes(sdk_payloads[1][0..2].try_into().unwrap()),
            EventKind::FrameMark as u16
        );
    }

    #[test]
    fn inject_flow_answers_via_plan_and_logs_once() {
        let mut l = log();
        let mem = channel_page();
        let plan = TableFaultPlan::new(vec![detguest_host::inject::FaultRule {
            name_glob: "io_*".into(),
            occurrence: None,
            decision: FaultDecision::Platform { kind: 1, arg: 5 },
        }]);
        let mut host = DetChannelHost::new(mem.clone(), plan);
        with_ctx(&mut l, 10, |ctx| {
            host.pio_out(PORT_INIT_LO, BASE as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), InitStatus::Ok as u32);
        });
        put_ring_w(
            &mem,
            &[
                EventPayload::NameIntern {
                    name_id: 1,
                    name: b"io_read",
                },
                EventPayload::InjectQuery {
                    iseq: 0,
                    name_id: 1,
                },
            ],
        );

        let before = l.record_count();
        let v = with_ctx(&mut l, 30, |ctx| {
            // OUT drains ring W inside the exit (sequencing rule)…
            host.pio_out(PORT_INJECT, 0, ctx);
            // …and IN answers for the latched iseq.
            host.pio_in(PORT_INJECT, ctx)
        });
        assert_eq!(v, FaultDecision::Platform { kind: 1, arg: 5 }.pack());
        // Drain logged CONS_BUMP + 2 SDK_EVENTs; the answer logged exactly
        // one PIO_ANSWER (via the responder's sink, not doubled).
        assert_eq!(l.record_count(), before + 4);

        // IN with no preceding OUT: deterministic Proceed + metric.
        let v2 = with_ctx(&mut l, 31, |ctx| host.pio_in(PORT_INJECT, ctx));
        assert_eq!(v2, 0);
        assert_eq!(host.metrics.inject_in_without_out, 1);

        // OUT for an iseq nobody queried: responder answers Proceed and
        // bumps the channel's unmatched metric.
        let v3 = with_ctx(&mut l, 32, |ctx| {
            host.pio_out(PORT_INJECT, 99, ctx);
            host.pio_in(PORT_INJECT, ctx)
        });
        assert_eq!(v3, 0);
        assert_eq!(host.channel().unwrap().unmatched_injects, 1);
    }

    #[test]
    fn quiesce_ack_latches_for_run_control() {
        let mut l = log();
        let mut host = attached(channel_page(), &mut l);
        with_ctx(&mut l, 40, |ctx| {
            host.pio_out(PORT_QUIESCE_ACK, 0xCAFE, ctx);
            assert_eq!(host.pio_in(PORT_QUIESCE_ACK, ctx), 0);
        });
        assert_eq!(host.take_quiesce_ack(), Some(0xCAFE));
        assert_eq!(host.take_quiesce_ack(), None);
    }

    #[test]
    fn drain_before_attach_is_empty_and_push_errors() {
        let mut l = log();
        let mut host = DetChannelHost::new(channel_page(), LogFaultPlan::default());
        with_ctx(&mut l, 1, |ctx| {
            assert!(host.drain_at_pause(ctx).is_empty());
            assert_eq!(
                host.push_command(
                    &Command::Quiesce {
                        token: 1,
                        mode: QuiesceMode::Coop
                    },
                    ctx
                ),
                Err(PushChannelError::NotAttached)
            );
        });
    }

    #[test]
    fn push_command_logs_ring_push_with_ring_c_id() {
        let mut l = log();
        let mut host = attached(channel_page(), &mut l);
        with_ctx(&mut l, 50, |ctx| {
            host.push_command(
                &Command::Quiesce {
                    token: 7,
                    mode: QuiesceMode::Coop,
                },
                ctx,
            )
            .unwrap();
        });
        let recs = records(&seal(l));
        let pushes: Vec<_> = recs
            .iter()
            .filter(|(k, p)| {
                *k == KIND_DEV_EVENT
                    && u16::from_le_bytes(p[2..4].try_into().unwrap()) == EVENT_RING_PUSH
            })
            .collect();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].1[8], 0, "ring id byte must be C=0");
        // RING_PUSH data (after the 8-byte DEV_EVENT preamble): ring id u8,
        // pad[3], new_prod u32 LE, then the pushed record bytes.
        let new_prod = u32::from_le_bytes(pushes[0].1[12..16].try_into().unwrap());
        assert!(new_prod > 0);
        assert!(pushes[0].1.len() > 16, "record bytes must follow");
    }

    #[test]
    fn pause_drain_equals_doorbell_drain() {
        let mut l = log();
        let mem = channel_page();
        let mut host = attached(mem.clone(), &mut l);
        put_ring_w(&mem, &[EventPayload::Beacon { beacon_id: 3 }]);
        let events = with_ctx(&mut l, 60, |ctx| host.drain_at_pause(ctx));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].payload,
            OwnedPayload::Beacon { beacon_id: 3 }
        ));
        assert_eq!(events[0].ring, RingId::W);
    }

    #[test]
    fn inject_latch_survives_across_exits() {
        // OUT 0xD384 and IN 0xD384 are SEPARATE VM exits; the latch must
        // carry the selected iseq between them (boot-scoped by design).
        let mut l = log();
        let mem = channel_page();
        let mut host = attached(mem.clone(), &mut l);
        put_ring_w(
            &mem,
            &[
                EventPayload::NameIntern {
                    name_id: 1,
                    name: b"io_read",
                },
                EventPayload::InjectQuery {
                    iseq: 4,
                    name_id: 1,
                },
            ],
        );
        with_ctx(&mut l, 70, |ctx| {
            host.pio_out(PORT_INJECT, 4, ctx);
        });
        // Much later, an unrelated exit INs the port: it answers for the
        // still-latched iseq 4 (LogFaultPlan answers Proceed = 0) and the
        // query is consumed, so it is not "unmatched".
        let v = with_ctx(&mut l, 90, |ctx| host.pio_in(PORT_INJECT, ctx));
        assert_eq!(v, 0);
        assert_eq!(host.channel().unwrap().unmatched_injects, 0);
        assert_eq!(host.metrics.inject_in_without_out, 0);
    }

    #[test]
    fn truncated_and_non_utf8_events_digest_identically() {
        use detguest_wire::events::MAX_DETAILS;
        // An over-cap details field: the SDK-side encoder clips it and sets
        // TRUNCATED on the wire. Non-UTF-8 name: raw bytes on the wire.
        let big = vec![0xABu8; MAX_DETAILS + 100];
        let evs = [
            EventPayload::NameIntern {
                name_id: 1,
                name: &[0xFF, 0xFE, 0x80],
            },
            EventPayload::AssertViolation {
                name_id: 1,
                violation_count: 1,
                details: &big,
            },
        ];

        let drain_once = || {
            let mut l = log();
            let mem = channel_page();
            let mut host = attached(mem.clone(), &mut l);
            put_ring_w(&mem, &evs);
            let events = with_ctx(&mut l, 80, |ctx| host.drain_at_pause(ctx));
            assert_eq!(host.metrics.sdk_digest_failures, 0);
            (events, seal(l))
        };
        let (events_a, sealed_a) = drain_once();
        let (events_b, sealed_b) = drain_once();

        // The truncated flag survives the drain; the lossy intern path and
        // the raw-bytes digest path coexist.
        assert!(events_a[1].truncated, "over-cap details must be TRUNCATED");
        assert!(matches!(
            &events_a[0].payload,
            OwnedPayload::NameIntern { name, .. } if name == &[0xFF, 0xFE, 0x80]
        ));
        assert_eq!(events_a, events_b);

        // Byte-identical guest state -> byte-identical SDK_EVENT records
        // (stream, len, digest8) on two independent hosts.
        let sdk = |sealed: &[u8]| -> Vec<Vec<u8>> {
            records(sealed)
                .into_iter()
                .filter(|(k, _)| *k == KIND_SDK_EVENT)
                .map(|(_, p)| p)
                .collect()
        };
        assert_eq!(sdk(&sealed_a), sdk(&sealed_b));
        assert_eq!(sdk(&sealed_a).len(), 2);
    }

    #[test]
    fn ring_a_drains_before_ring_w_with_correct_ids() {
        let mut l = log();
        let mem = channel_page();
        let mut host = attached(mem.clone(), &mut l);
        put_ring(
            &mem,
            RingId::A,
            &[EventPayload::Hello {
                proto_version: 1,
                agent_version: 1,
                capabilities: 0,
            }],
        );
        put_ring(&mem, RingId::W, &[EventPayload::Beacon { beacon_id: 9 }]);

        let events = with_ctx(&mut l, 100, |ctx| {
            host.pio_out(PORT_DOORBELL, DOORBELL_RING_A | DOORBELL_RING_W, ctx)
        });
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].ring, RingId::A, "ring A drains first");
        assert_eq!(events[1].ring, RingId::W);

        let recs = records(&seal(l));
        let cons_ids: Vec<u8> = recs
            .iter()
            .filter(|(k, p)| {
                *k == KIND_DEV_EVENT
                    && u16::from_le_bytes(p[2..4].try_into().unwrap()) == EVENT_CONS_BUMP
            })
            .map(|(_, p)| p[8])
            .collect();
        assert_eq!(cons_ids, vec![2, 3], "CONS_BUMP ring ids: A=2 then W=3");
    }

    #[test]
    fn corrupt_ring_indices_fail_the_drain_deterministically() {
        let mut l = log();
        let mem = channel_page();
        let mut host = attached(mem.clone(), &mut l);
        // used = prod - cons > ring size: wire-level corruption.
        mem.0
            .borrow_mut()
            .write(
                BASE + RingId::W.prod_offset() as u64,
                &0xFFFF_FFF0u32.to_le_bytes(),
            )
            .unwrap();
        let events = with_ctx(&mut l, 110, |ctx| host.drain_at_pause(ctx));
        assert!(events.is_empty());
        assert_eq!(host.metrics.drain_failures, 1);
        assert!(host.last_drain_error.is_some());
        assert!(host.metrics.any_anomaly());
    }

    #[test]
    fn ring_c_full_surfaces_ring_full_for_pause_retry() {
        let mut l = log();
        let mut host = attached(channel_page(), &mut l);
        let cmd = Command::Quiesce {
            token: 1,
            mode: QuiesceMode::Coop,
        };
        // Ring C is finite; pushing without the guest consuming must end in
        // RingFull (the host retries at the next pause, never spins).
        let mut full = None;
        for i in 0..100_000 {
            let r = with_ctx(&mut l, 200 + i, |ctx| host.push_command(&cmd, ctx));
            if let Err(e) = r {
                full = Some(e);
                break;
            }
        }
        assert!(matches!(
            full,
            Some(PushChannelError::Push(detguest_host::PushError::RingFull))
        ));
        // Producer seqs advanced and are exposed for checkpointing.
        assert!(host.producer_seqs().unwrap().ring_c > 0);
    }
}

#[cfg(test)]
mod evtc_tests {
    use super::*;
    use crate::ctx::test_support::FakeEntropy;
    use crate::ctx::VecGuestMem;
    use detguest_host::{LogFaultPlan, MemError, MockGuestMem};
    use detguest_wire::events::QuiesceMode;
    use detguest_wire::header::{ChannelHeader, CHANNEL_SIZE, OFF_MANIFEST, OFF_RESERVED};
    use detguest_wire::manifest::{init_manifest, MANIFEST_TOTAL_SIZE};
    use detguest_wire::Command;
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
    use std::cell::RefCell;
    use std::rc::Rc;

    const BASE: u64 = 0x1000_0000;

    #[derive(Clone)]
    struct SharedMem(Rc<RefCell<MockGuestMem>>);
    impl GuestMem for SharedMem {
        fn read(&self, gpa: u64, buf: &mut [u8]) -> Result<(), MemError> {
            self.0.borrow().read(gpa, buf)
        }
        fn write(&mut self, gpa: u64, buf: &[u8]) -> Result<(), MemError> {
            self.0.borrow_mut().write(gpa, buf)
        }
    }

    fn channel_page() -> SharedMem {
        let mut gm = MockGuestMem::with_zeroed(BASE, CHANNEL_SIZE);
        let mut hdr = [0u8; OFF_RESERVED];
        ChannelHeader::canonical().write_to(&mut hdr).unwrap();
        gm.write(BASE, &hdr).unwrap();
        let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
        init_manifest(&mut area).unwrap();
        gm.write(BASE + OFF_MANIFEST as u64, &area).unwrap();
        SharedMem(Rc::new(RefCell::new(gm)))
    }

    fn log() -> LogWriter {
        LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
        })
    }

    fn with_ctx<R>(l: &mut LogWriter, icount: u64, f: impl FnOnce(&mut DevCtx) -> R) -> R {
        let mut mem = VecGuestMem(vec![0u8; 16]);
        let mut ent = FakeEntropy(0);
        let mut q = Vec::new();
        let mut ctx = DevCtx::new(icount, 0, l, &mut mem, &mut ent, &mut q);
        f(&mut ctx)
    }

    #[test]
    fn evtc_roundtrips_attached_state_and_seqs() {
        let mut l = log();
        let mem = channel_page();
        let mut host = DetChannelHost::new(mem.clone(), LogFaultPlan::default());
        with_ctx(&mut l, 10, |ctx| {
            host.pio_out(PORT_INIT_LO, BASE as u32, ctx);
            host.pio_out(PORT_INIT_GO, CHANNEL_SIZE_PAGES, ctx);
            assert_eq!(host.pio_in(PORT_INIT_GO, ctx), 0);
            // Consume ring C seqs 0 and 1 (two pushes).
            for token in [1, 2] {
                host.push_command(
                    &Command::Quiesce {
                        token,
                        mode: QuiesceMode::Coop,
                    },
                    ctx,
                )
                .unwrap();
            }
            host.pio_out(PORT_QUIESCE_ACK, 0xBEEF, ctx);
            host.pio_out(PORT_INJECT, 7, ctx); // latch an iseq
        });
        let seqs_before = host.producer_seqs().unwrap();
        assert!(seqs_before.ring_c >= 2);

        let mut section = Vec::new();
        host.snapshot(&mut section);
        assert_eq!(
            section.len(),
            DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_LEN
        );

        // Fresh host over the SAME guest RAM (the restore precondition:
        // RAM is already back).
        let mut restored = DetChannelHost::new(mem, LogFaultPlan::default());
        restored
            .restore(
                &section,
                DetChannelHost::<SharedMem, LogFaultPlan>::EVTC_VERSION,
            )
            .unwrap();
        assert_eq!(restored.channel_gpa(), Some(BASE));
        assert_eq!(restored.producer_seqs().unwrap(), seqs_before);
        assert!(restored.manifest().is_some(), "manifest re-read at attach");
        assert_eq!(restored.take_quiesce_ack(), Some(0xBEEF));
        let mut l2 = log();
        with_ctx(&mut l2, 20, |ctx| {
            assert_eq!(restored.pio_in(PORT_INIT_LO, ctx), BASE as u32);
            assert_eq!(restored.pio_in(PORT_INIT_GO, ctx), 0, "status restored");
        });

        // The next push uses the NEXT seq — never a reused one.
        let mut l3 = log();
        with_ctx(&mut l3, 30, |ctx| {
            restored
                .push_command(
                    &Command::Quiesce {
                        token: 3,
                        mode: QuiesceMode::Coop,
                    },
                    ctx,
                )
                .unwrap();
        });
        assert_eq!(
            restored.producer_seqs().unwrap().ring_c,
            seqs_before.ring_c + 1
        );
    }

    #[test]
    fn evtc_roundtrips_detached_state_and_refuses_bad_input() {
        let host: DetChannelHost<SharedMem, LogFaultPlan> =
            DetChannelHost::new(channel_page(), LogFaultPlan::default());
        let mut section = Vec::new();
        host.snapshot(&mut section);

        let mut restored = DetChannelHost::new(channel_page(), LogFaultPlan::default());
        restored.restore(&section, 1).unwrap();
        assert!(restored.channel().is_none());
        assert_eq!(restored.channel_gpa(), None);

        assert!(restored.restore(&section, 2).is_err(), "wrong version");
        assert!(restored.restore(&section[..10], 1).is_err(), "truncated");

        // Attached flag with a GPA that does not hold a valid channel
        // (zeroed RAM) must refuse loudly, not attach garbage.
        let mut bad = section.clone();
        bad[22] = 1;
        bad[23..31].copy_from_slice(&BASE.to_le_bytes());
        let mut h2: DetChannelHost<SharedMem, LogFaultPlan> = DetChannelHost::new(
            SharedMem(Rc::new(RefCell::new(MockGuestMem::with_zeroed(
                BASE,
                CHANNEL_SIZE,
            )))),
            LogFaultPlan::default(),
        );
        assert!(h2.restore(&bad, 1).is_err(), "bad header at GPA refuses");
    }
}
