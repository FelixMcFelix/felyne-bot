use crate::constants::TRACE_DIR;
use dashmap::DashMap;
use flume::{Receiver, Sender, TryRecvError};
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use songbird::{
	events::{CoreEvent, Event, EventContext, EventHandler},
	Call,
};
use std::{
	collections::hash_map::HashMap,
	fs::{self, File},
	mem,
	num::NonZeroU16,
	sync::Arc,
	thread,
	time::{SystemTime, UNIX_EPOCH},
};
use tracing::*;

type Ssrc = u32;
type Uid = u64;

#[derive(Clone)]
pub struct VoiceHuntReceiver {
	sessions: Arc<DashMap<Ssrc, VoiceHuntSession>>,
	user_map: Arc<DashMap<Uid, Ssrc>>,
	rx: Receiver<ReceiverSignal>,
	tx: Sender<ReceiverSignal>,
}

impl VoiceHuntReceiver {
	pub fn new() -> Self {
		let (tx, rx) = flume::bounded(1);
		Self {
			sessions: Arc::new(Default::default()),
			user_map: Arc::new(Default::default()),
			rx,
			tx,
		}
	}

	fn try_read_poison(&self) -> bool {
		match self.rx.try_recv() {
			Ok(ReceiverSignal::Poison) | Err(TryRecvError::Disconnected) => {
				let _ = self.tx.send(ReceiverSignal::Poison);
				true
			},
			Err(TryRecvError::Empty) => false,
		}
	}
}

#[async_trait]
impl EventHandler for VoiceHuntReceiver {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
		if self.try_read_poison() {
			Some(Event::Cancel)
		} else {
			match ctx {
				EventContext::SpeakingStateUpdate(s) => {
					let _ = self
						.sessions
						.entry(s.ssrc)
						.or_insert_with(|| VoiceHuntSession::new());

					if let Some(u_id) = &s.user_id {
						let _ = self
							.user_map
							.entry(u_id.0)
							.or_insert_with(|| s.ssrc);
					}

					None
				},
				EventContext::VoicePacket {
					audio,
					packet,
					payload_offset,
					payload_end_pad,
				} => {
					println!("RTP");
					let ssrc = packet.ssrc;

					let mut entry = self
						.sessions
						.entry(ssrc)
						.or_insert_with(|| VoiceHuntSession::new());

					entry.value_mut().record(
						packet.timestamp.into(),
						packet.sequence.into(),
						packet.payload.len() - payload_offset,
					);

					None
				},
				EventContext::RtcpPacket { .. } => {
					println!("RTCP");
					None
				},
				EventContext::ClientConnect(s) => {
					let _ = self
						.sessions
						.entry(s.audio_ssrc)
						.or_insert_with(|| VoiceHuntSession::new());

					let _ = self
						.user_map
						.entry(s.user_id.0)
						.or_insert_with(|| s.audio_ssrc);

					None
				},
				EventContext::ClientDisconnect(s) => {
					if let Some((k, v)) = self.user_map.remove(&s.user_id.0) {
						if let Some((ssrc, sess)) = self.sessions.remove(&v) {
							finalise_audio_session(sess.clone());
						}
					}

					None
				},
				EventContext::SpeakingUpdate { .. } => None,
				_ => None,
			}
		}
	}
}

impl Drop for VoiceHuntReceiver {
	fn drop(&mut self) {
		// Easier to do than having a synchro mechanism.
		// Joining a new channel will cause the old receiver to be dropped.
		// Similarly, leaving completely eill do the same...
		self.sessions.iter().for_each(|guard| {
			let (_ssrc, sess) = guard.pair();
			finalise_audio_session(sess.clone())
		});
	}
}

#[derive(Clone, Copy, Debug)]
struct VoicePacketMetadata {
	timestamp: u32,
	sequence: u16,
	size: NonZeroU16,
}

#[derive(Clone, Debug)]
struct VoiceHuntSession {
	packets: Vec<VoicePacketMetadata>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum PacketChainLink {
	Packet(NonZeroU16),
	Missing(u16),
	Silence(u32),
}

impl VoiceHuntSession {
	fn new() -> Self {
		Self { packets: vec![] }
	}

	fn record(&mut self, timestamp: u32, sequence: u16, pkt_size: usize) {
		let pkt = VoicePacketMetadata {
			timestamp,
			sequence,
			size: NonZeroU16::new(pkt_size as u16).expect("Minimum body size is 3 bytes."),
		};

		// rough idea:
		// if timestamp is at risk of overflow, then finalise the existing
		// session and start a new one.
		let last_time = self
			.packets
			.last()
			.map(|x| x.timestamp)
			.unwrap_or(timestamp);
		let reduced = timestamp < last_time;

		if reduced && last_time - timestamp > 2_000_000 {
			let mut replacement = Self::new();
			mem::swap(&mut self.packets, &mut replacement.packets);

			// mem::swap(self, &mut replacement);
			finalise_audio_session(replacement);
		}

		self.packets.push(pkt);
	}

	fn finalise(&mut self) {
		let mut output: Vec<PacketChainLink> = vec![];
		let mut last_packet: Option<VoicePacketMetadata> = None;

		// Key assumption: timestamp shouldn't
		// overflow until a long, long time in the future.
		// sorting here is cheaper than maybe inserting (and displacing elements)
		// every time.
		self.packets.sort_unstable_by(|a, b| {
			a.timestamp
				.cmp(&b.timestamp)
				.then_with(|| a.sequence.cmp(&b.sequence).reverse())
		});

		for packet in &self.packets {
			trace!("Packet with len: {:?}", packet);
			let update = if let Some(pkt_old) = last_packet {
				// gaps in sequence are dropped packets.
				let mut target_sequence = pkt_old.sequence.wrapping_add(1);
				let mut dropped_packets = 0;

				if packet.timestamp <= pkt_old.timestamp {
					// Anomalous behaviour: we might get a slew of packets
					// with the same timestamp but outdated sequence.
					// What does this look like?
					// Multiple packets w/ same timestamp, bundled with the true next packet.

					// Iterate backwards through output to find/replace a matching missing packet.
					let mut iter = output.iter_mut();
					let mut quit = false;
					loop {
						if quit {
							break;
						}

						if let Some(elem) = iter.next_back() {
							use PacketChainLink::*;

							*elem = match elem {
								Missing(x) if *x == packet.sequence => {
									quit = true;
									Packet(packet.size)
								},
								_ => *elem,
							};
						} else {
							// Made it all the way back without replacing: insert.
							output.insert(0, PacketChainLink::Packet(packet.size));
							break;
						}
					}

					// early exit to prevent packet insertion
					continue;
				}

				// seems to be one case of pathological behaviour which can occur,
				// where a packet with later timestamp has a smaller sequence (non_wrap).
				let standard_order = packet.sequence >= target_sequence;

				if standard_order {
					while target_sequence != packet.sequence {
						if dropped_packets == 0 {
							info!(
								"Expected {:?}, making up for {:?}",
								packet.sequence, target_sequence
							);
						}
						output.push(PacketChainLink::Missing(target_sequence));
						target_sequence = target_sequence.wrapping_add(1);
						dropped_packets += 1;
					}

					if dropped_packets != 0 {
						info!(
							"Pushed {} missing packets from {} to {}.",
							dropped_packets,
							pkt_old.sequence.wrapping_add(1),
							target_sequence
						);
					}
				}

				// timestamp gaps larger than 960 samples are silent breaks.
				// note: 960 samples == 20ms
				let t_diff = packet.timestamp.wrapping_sub(pkt_old.timestamp) / 48;
				let windows = (dropped_packets + 1) * 20;
				if t_diff > windows {
					output.push(PacketChainLink::Silence(t_diff - windows));
				}

				standard_order
			} else {
				true
			};

			// if sequence order was violated (against timestamp order), then
			// concat that packet but don't treat it as the most recent.
			output.push(PacketChainLink::Packet(packet.size));
			if update {
				last_packet = Some(*packet);
			}
		}

		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

		let mut attempt = 0;
		let orig_fname = format!("{}", now.as_secs() * 1000 + u64::from(now.subsec_millis()));
		let mut fname = orig_fname.clone();

		let _ = fs::create_dir_all(TRACE_DIR);

		loop {
			if let Ok(mut file) = File::create(format!("{}{}", TRACE_DIR, fname)) {
				let _ = bincode::serialize_into(&mut file, &output);
				break;
			} else {
				fname = format!("{}-{}", orig_fname, attempt);
				attempt += 1;
			}
		}
	}
}

fn finalise_audio_session(mut a: VoiceHuntSession) {
	thread::spawn(move || a.finalise());
}

pub enum ReceiverSignal {
	Poison,
}

pub fn listen_in(handler: &mut Call) -> Sender<ReceiverSignal> {
	let vhr = VoiceHuntReceiver::new();
	let out_tx = vhr.tx.clone();

	let n_vhr = vhr.clone();
	handler.add_global_event(CoreEvent::SpeakingStateUpdate.into(), n_vhr);

	let n_vhr = vhr.clone();
	handler.add_global_event(CoreEvent::VoicePacket.into(), n_vhr);

	let n_vhr = vhr.clone();
	handler.add_global_event(CoreEvent::RtcpPacket.into(), n_vhr);

	let n_vhr = vhr.clone();
	handler.add_global_event(CoreEvent::ClientConnect.into(), n_vhr);

	let n_vhr = vhr.clone();
	handler.add_global_event(CoreEvent::ClientDisconnect.into(), n_vhr);

	handler.add_global_event(CoreEvent::SpeakingUpdate.into(), vhr);

	out_tx
}
