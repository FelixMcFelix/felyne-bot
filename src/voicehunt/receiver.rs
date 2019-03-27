use crate::constants::TRACE_DIR;

use bincode;
use serde::{
	Deserialize,
	Serialize,
};
use serenity::voice::AudioReceiver;
use std::{
	collections::hash_map::HashMap,
	fs::{
		self,
		File,
	},
	mem,
	num::NonZeroU16,
	thread,
	time::{
		UNIX_EPOCH,
		SystemTime,
	},
};

pub struct VoiceHuntReceiver {
	sessions: HashMap<u32, VoiceHuntSession>,
	user_map: HashMap<u64, u32>,
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
		self.sessions.drain()
			.for_each(|(_ssrc, sess)| finalise_audio_session(sess));
	}
}

#[derive(Clone, Copy, Debug)]
struct VoicePacketMetadata {
	timestamp: u32,
	sequence: u16,
	size: NonZeroU16,
}

#[derive(Debug)]
struct VoiceHuntSession {
	packet_chain: Vec<PacketChainLink>,
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
			packet_chain: vec![],
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
			mem::swap(&mut self.packet_chain, &mut replacement.packet_chain);
			mem::swap(&mut self.packets, &mut replacement.packets);

			// mem::swap(self, &mut replacement);
			finalise_audio_session(replacement);
		}

		self.packets.push(pkt);
		// println!("Now at {:?}", self.packets.len());
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
			println!("{:?}", packet);
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
							println!("Expected {:?}, making up for {:?}", packet.sequence, target_sequence);
						}
						output.push(PacketChainLink::Missing(target_sequence));
						target_sequence = target_sequence.wrapping_add(1);
						dropped_packets += 1;
					}

					if dropped_packets != 0 {
						println!("Pushed {} missing packets from {} to {}.", dropped_packets, pkt_old.sequence.wrapping_add(1), target_sequence);
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

impl AudioReceiver for VoiceHuntReceiver {
	fn speaking_update(&mut self, ssrc: u32, user_id: u64, _speaking: bool) {
		// println!("User {} (sess {}) is now speaking? {}", user_id, ssrc, _speaking);
		let _ = self.sessions.entry(ssrc).or_insert_with(VoiceHuntSession::new);
		let _ = self.user_map.entry(user_id).or_insert(ssrc);
	}

	fn voice_packet(&mut self, ssrc: u32, sequence: u16, timestamp: u32, _stereo: bool, _data: &[i16], compressed_size: usize) {
		// println!("pkt from {:?}/{}", ssrc, sequence);
		let sess = self.sessions.entry(ssrc).or_insert_with(VoiceHuntSession::new);
		sess.record(timestamp, sequence, compressed_size);
	}

	fn client_connect(&mut self, ssrc: u32, user_id: u64) {
		// println!("User {} connected to session {}", user_id, ssrc);
		let _ = self.user_map.entry(user_id).or_insert(ssrc);
		let _ = self.sessions.entry(ssrc).or_insert_with(VoiceHuntSession::new);
	}

	fn client_disconnect(&mut self, user_id: u64) {
		// println!("User {} disconnected", user_id);
		if let Some(ssrc) = self.user_map.remove(&user_id) {
			if let Some(sess) = self.sessions.remove(&ssrc) {
				// must be certain that we'er not hanging the audio thread...
				finalise_audio_session(sess);
			}
		}
	}
}
