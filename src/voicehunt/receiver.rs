use crate::constants::TRACE_DIR;
use crossbeam::channel::{
	self,
	Receiver,
	Sender,
	TryRecvError,
};
use dashmap::DashMap;
use log::*;
use serde::{
	Deserialize,
	Serialize,
};
use serenity::voice::{
	events::{CoreEvent, Event, EventContext},
	Handler,
};
use std::{
	collections::hash_map::HashMap,
	fs::{
		self,
		File,
	},
	mem,
	num::NonZeroU16,
	sync::Arc,
	thread,
	time::{
		UNIX_EPOCH,
		SystemTime,
	},
};

type Ssrc = u32;
type Uid = u64;

pub struct VoiceHuntReceiver {
	sessions: DashMap<Ssrc, VoiceHuntSession>,
	user_map: DashMap<Uid, Ssrc>,
}

impl VoiceHuntReceiver {
	pub fn new() -> Self { 
		Self { 
			sessions: Default::default(),
			user_map: Default::default(),
		}
	}
}

impl Drop for VoiceHuntReceiver {
	fn drop(&mut self) {
		// Easier to do than having a synchro mechanism.
		// Joining a new channel will cause the old receiver to be dropped.
		// Similarly, leaving completely eill do the same...
		self.sessions.iter()
			.for_each(|guard| {
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
	packets: Vec<VoicePacketMetadata>
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum PacketChainLink {
	Packet(NonZeroU16),
	Missing(u16),
	Silence(u32),
}

impl VoiceHuntSession {
	fn new() -> Self {
		Self { 
			packets: vec![],
		}
	}

	fn record(&mut self, timestamp: u32, sequence: u16, pkt_size: usize) {
		let pkt = VoicePacketMetadata {
			timestamp,
			sequence,
			size: NonZeroU16::new(pkt_size as u16)
				.expect("Minimum body size is 3 bytes."),
		};

		// rough idea:
		// if timestamp is at risk of overflow, then finalise the existing
		// session and start a new one.
		let last_time = self.packets.last()
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
		self.packets.sort_unstable_by(|a, b|
			a.timestamp.cmp(&b.timestamp)
				.then_with(|| a.sequence.cmp(&b.sequence).reverse())
		);

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
							info!("Expected {:?}, making up for {:?}", packet.sequence, target_sequence);
						}
						output.push(PacketChainLink::Missing(target_sequence));
						target_sequence = target_sequence.wrapping_add(1);
						dropped_packets += 1;
					}

					if dropped_packets != 0 {
						info!("Pushed {} missing packets from {} to {}.", dropped_packets, pkt_old.sequence.wrapping_add(1), target_sequence);
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

fn try_read_poison(rx: &Receiver<ReceiverSignal>, tx: &Sender<ReceiverSignal>) -> bool {
	match rx.try_recv() {
		Ok(ReceiverSignal::Poison) | Err(TryRecvError::Disconnected) => {
			let _ = tx.send(ReceiverSignal::Poison);
			true
		},
		Err(TryRecvError::Empty) => {
			false
		},
	}
}

pub fn listen_in(handler: &mut Handler) -> Sender<ReceiverSignal> {
	let vhr = Arc::new(VoiceHuntReceiver::new());
	let (tx, rx) = channel::bounded(1);

	let n_tx = tx.clone();
	let n_rx = rx.clone();
	let n_vhr = vhr.clone();
	handler.add_global_event(
		CoreEvent::SpeakingStateUpdate.into(),
		move |ctx| {
			if try_read_poison(&n_rx, &n_tx) {
				Some(Event::Cancel)
			} else if let EventContext::SpeakingStateUpdate(s) = ctx {
				if n_vhr.sessions.get(&s.ssrc).is_none() {
					n_vhr.sessions.insert(s.ssrc, VoiceHuntSession::new());
				}
				if n_vhr.user_map.get(&s.user_id.0).is_none() {
					n_vhr.user_map.insert(s.user_id.0, s.ssrc);
				}

				None
			} else {
				None
			}
		}
	);

	let n_tx = tx.clone();
	let n_rx = rx.clone();
	let n_vhr = vhr.clone();
	handler.add_global_event(
		CoreEvent::VoicePacket.into(),
		move |ctx| {
			if try_read_poison(&n_rx, &n_tx) {
				Some(Event::Cancel)
			} else if let EventContext::VoicePacket {audio, packet, payload_offset} = ctx {
				let ssrc = packet.ssrc;

				if n_vhr.sessions.get(&ssrc).is_none() {
					n_vhr.sessions.insert(ssrc, VoiceHuntSession::new());
				}

				n_vhr.sessions.update(&ssrc, move |ssrc, sess| {
					let mut n_sess = sess.clone();
					n_sess.record(packet.timestamp.into(), packet.sequence.into(), packet.payload.len() - payload_offset);
					n_sess
				});

				None
			} else {
				None
			}
		}
	);

	let n_tx = tx.clone();
	let n_rx = rx.clone();
	handler.add_global_event(
		CoreEvent::RtcpPacket.into(),
		move |ctx| {
			if try_read_poison(&n_rx, &n_tx) {
				Some(Event::Cancel)
			} else if let EventContext::RtcpPacket {..} = ctx {
				None
			} else {
				None
			}
		}
	);

	let n_tx = tx.clone();
	let n_rx = rx.clone();
	let n_vhr = vhr.clone();
	handler.add_global_event(
		CoreEvent::ClientConnect.into(),
		move |ctx| {
			if try_read_poison(&n_rx, &n_tx) {
				Some(Event::Cancel)
			} else if let EventContext::ClientConnect(s) = ctx {
				if n_vhr.sessions.get(&s.audio_ssrc).is_none() {
					n_vhr.sessions.insert(s.audio_ssrc, VoiceHuntSession::new());
				}
				if n_vhr.user_map.get(&s.user_id.0).is_none() {
					n_vhr.user_map.insert(s.user_id.0, s.audio_ssrc);
				}

				None
			} else {
				None
			}
		}
	);

	let n_tx = tx.clone();
	let n_rx = rx.clone();
	let n_vhr = vhr;
	handler.add_global_event(
		CoreEvent::ClientDisconnect.into(),
		move |ctx| {
			if try_read_poison(&n_rx, &n_tx) {
				Some(Event::Cancel)
			} else if let EventContext::ClientDisconnect(s) = ctx {
				if let Some(guard) = n_vhr.user_map.remove_take(&s.user_id.0) {
					let (k, v) = guard.pair();
					if let Some(guard) = n_vhr.sessions.remove_take(v) {
						let (ssrc, sess) = guard.pair();
						finalise_audio_session(sess.clone());
					}
				}

				None
			} else {
				None
			}
		}
	);

	let n_tx = tx.clone();
	let n_rx = rx;
	handler.add_global_event(
		CoreEvent::SpeakingUpdate.into(),
		move |ctx| {
			if try_read_poison(&n_rx, &n_tx) {
				Some(Event::Cancel)
			} else if let EventContext::SpeakingUpdate{..} = ctx {
				None
			} else {
				None
			}
		}
	);

	tx
}
