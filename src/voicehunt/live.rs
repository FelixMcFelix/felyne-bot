use crate::{guild::*, server::Label, UserState};
use felyne_trace::{traces::FelyneTraceV2, *};
use serenity::{client::Context, model::prelude::UserId};
use songbird::{
	model::payload::Speaking,
	packet::{
		demux::{self, DemuxedMut},
		rtcp::{report::*, MutableRtcpPacket, Rtcp},
		rtp::{Rtp, RtpExtensionPacket},
		MutablePacket,
		PacketSize,
	},
};
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::Arc,
	time::Instant,
};
use tokio::sync::RwLock;
use tracing::warn;

pub type LocalTimedEvent = (Instant, Event);

// Idea: convert to a single stream of events by drawing from the front of each deque
// according to who is "first" a la merge sort.
#[derive(Clone)]
pub struct LiveTrace {
	rtcps: VecDeque<LocalTimedEvent>,
	my_ssrcs: Vec<u32>,
	my_uid: Option<UserId>,
	servers: VecDeque<(Instant, String)>,
	region_override: Option<String>,
	start_time: Instant,
	user_streams: HashMap<u32, VecDeque<LocalTimedEvent>>,
	lost_events: VecDeque<(u64, LocalTimedEvent)>,
	label: Label,
	first_measures: HashMap<u32, (u16, u32)>, //first seq/ts
	ssrc_to_user: HashMap<u32, UserId>,
	user_to_ssrcs: HashMap<UserId, Vec<u32>>,
	users_at_start: usize,
}

impl LiveTrace {
	pub fn new(
		start_time: Instant,
		label: Label,
		users_at_start: usize,
		region_override: Option<String>,
		my_uid: Option<UserId>,
	) -> Self {
		Self {
			start_time,
			label,
			my_ssrcs: vec![],
			my_uid,
			region_override,
			servers: Default::default(),
			rtcps: Default::default(),
			user_streams: Default::default(),
			lost_events: Default::default(),
			first_measures: Default::default(),
			ssrc_to_user: Default::default(),
			user_to_ssrcs: Default::default(),
			users_at_start,
		}
	}

	fn register_ssrc_userid(&mut self, ssrc: u32, user_id: UserId) {
		self.ssrc_to_user.insert(ssrc, user_id);
		self.user_to_ssrcs.entry(user_id).or_default().push(ssrc);
	}

	fn push_event(&mut self, time: Instant, ssrc: u32, evt: Event) {
		let entry = self.user_streams.entry(ssrc).or_default();

		entry.push_back((time, evt));
	}

	fn push_ssrcless_event(&mut self, time: Instant, uid: u64, evt: Event) {
		self.lost_events.push_back((uid, (time, evt)));
	}

	fn get_seq_ts_floor(&mut self, packet: &Rtp) -> (u16, u32) {
		*self
			.first_measures
			.entry(packet.ssrc)
			.or_insert(((packet.sequence.0).0, (packet.timestamp.0).0))
	}

	pub fn add_my_ssrc(&mut self, ssrc: u32) {
		self.my_ssrcs.push(ssrc);
	}

	pub fn change_server(&mut self, time: Instant, server: String) {
		self.servers.push_back((time, server));
	}

	pub fn speaking_state(&mut self, time: Instant, update: &Speaking) {
		if let Some(u_id) = update.user_id {
			self.register_ssrc_userid(update.ssrc, UserId::new(u_id.0));

			self.push_event(
				time,
				update.ssrc,
				Event::SpeakState(u_id.0, update.speaking.bits()),
			)
		}
	}

	pub fn packet(
		&mut self,
		time: Instant,
		packet: &Rtp,
		payload_offset: usize,
		payload_end_pad: usize,
	) {
		let (seq_floor, ts_floor) = self.get_seq_ts_floor(packet);

		let sequence = (packet.sequence.0).0.wrapping_sub(seq_floor);
		let timestamp = (packet.timestamp.0).0.wrapping_sub(ts_floor);

		let mut bytes_wasted = 0;

		let extension = if packet.extension != 0 {
			bytes_wasted += RtpExtensionPacket::minimum_packet_size();

			if let Some(ext) = RtpExtensionPacket::new(&packet.payload[payload_offset..]) {
				let bytes_occupied = (4 * ext.get_length()) as usize;
				bytes_wasted += bytes_occupied;
				// Some((ext.get_info(), bytes_occupied))
				let info = ext.get_info();

				let top_ext = TopExtension {
					info,
					length: bytes_occupied,
				};

				Some(match info {
					0xBEDE => {
						let mut extensions = vec![];

						let mut cursor = 0;
						let payload = ext.get_ext_data_raw();

						while cursor < bytes_occupied {
							let one_byte = payload[cursor];
							cursor += 1;

							if one_byte == 0 {
								continue;
							}

							let id = one_byte >> 4;
							let seen_length = one_byte & 0b1111;

							// rfc 8285, s4.1.2
							let length = seen_length + 1;

							let body = if felyne_trace::SAFE_SUB_EXTENSIONS.contains(&id) {
								payload[cursor..cursor + (length as usize)].to_vec()
							} else {
								vec![]
							};

							// Record anomalous entries, THEN stop.
							extensions.push(SubExtension { id, length, body });

							if id == 15 || (id == 0 && seen_length > 0) {
								break;
							}

							cursor += length as usize;
						}

						Extension::OneByte(top_ext, extensions)
					},
					a if (a >> 4) == (0x100) => {
						let mut extensions = vec![];

						let mut cursor = 0;
						let payload = ext.get_ext_data_raw();

						// need to read 2.
						while cursor < (bytes_occupied - 1) {
							let id = payload[cursor];
							cursor += 1;

							if id == 0 {
								continue;
							}

							let length = payload[cursor];
							cursor += 1;

							let body = if felyne_trace::SAFE_SUB_EXTENSIONS.contains(&id) {
								payload[cursor..cursor + (length as usize)].to_vec()
							} else {
								vec![]
							};

							extensions.push(SubExtension { id, length, body });

							cursor += length as usize;
						}

						Extension::TwoByte(top_ext, extensions)
					},
					a if felyne_trace::SAFE_TOP_EXTENSIONS.contains(&a) =>
						Extension::Standard(top_ext, ext.get_ext_data()),
					_ => Extension::Standard(top_ext, vec![]),
				})
			} else {
				eprintln!("Failed to get enough bytes?!");
				None
			}
		} else {
			None
		};

		let evt = Event::Packet {
			sender_id: packet.ssrc as u64,
			sequence,
			timestamp,
			audio_bytes: packet.payload.len() - payload_offset - payload_end_pad - bytes_wasted,
			extension,
		};

		self.push_event(time, packet.ssrc, evt);
	}

	pub fn rtcp(
		&mut self,
		time: Instant,
		packet: &Rtcp,
		payload_offset: usize,
		payload_end_pad: usize,
	) {
		let wasted_bytes = payload_offset + payload_end_pad;

		let bytes = match packet {
			Rtcp::SenderReport(sr) => {
				let needed = MutableSenderReportPacket::minimum_packet_size() + sr.payload.len()
					- wasted_bytes;
				let mut bytes = vec![0u8; needed];

				{
					let mut pkt_view = MutableSenderReportPacket::new(&mut bytes[..])
						.expect("I know there are enough bytes");

					// populate appears to be unsound? Check underlying library...
					pkt_view.set_version(sr.version);
					pkt_view.set_padding(sr.padding);
					pkt_view.set_rx_report_count(sr.rx_report_count);
					pkt_view.set_packet_type(sr.packet_type);
					pkt_view.set_pkt_length(sr.pkt_length);
					pkt_view.set_ssrc(sr.ssrc);

					let right_edge = sr.payload.len() - payload_end_pad;
					pkt_view.set_payload(&sr.payload[payload_offset..right_edge]);
				}

				Some(bytes)
			},
			Rtcp::ReceiverReport(rr) => {
				let needed = MutableReceiverReportPacket::minimum_packet_size() + rr.payload.len()
					- wasted_bytes;
				let mut bytes = vec![0u8; needed];

				{
					let mut pkt_view = MutableReceiverReportPacket::new(&mut bytes[..])
						.expect("I know there are enough bytes");

					// populate appears to be unsound? Check underlying library...
					pkt_view.set_version(rr.version);
					pkt_view.set_padding(rr.padding);
					pkt_view.set_rx_report_count(rr.rx_report_count);
					pkt_view.set_packet_type(rr.packet_type);
					pkt_view.set_pkt_length(rr.pkt_length);
					pkt_view.set_ssrc(rr.ssrc);

					let right_edge = rr.payload.len() - payload_end_pad;
					pkt_view.set_payload(&rr.payload[payload_offset..right_edge]);
				}

				Some(bytes)
			},
			Rtcp::KnownType(kt) => {
				warn!("Songbird can't decode RTCP type {:?}.", kt);
				None
			},
			_ => None,
		};

		if let Some(bytes) = bytes {
			let evt = Event::RtcpData(bytes);

			self.rtcps.push_back((time, evt));
		}
	}

	pub fn client_connect(&mut self, time: Instant, ssrc: u32, user_id: UserId) {
		self.register_ssrc_userid(ssrc, UserId::new(user_id.get()));

		self.push_event(time, ssrc, Event::Connect(user_id.get()))
	}

	pub fn client_disconnect(&mut self, time: Instant, user_id: UserId) {
		let evt = Event::Disconnect(user_id.get());

		let ssrc_to_push = if let Some(ssrc_list) = self.user_to_ssrcs.get(&user_id) {
			let ssrc = ssrc_list
				.last()
				.expect("Cannot have list without at least one ssrc!");

			Some(*ssrc)
		} else {
			None
		};

		if let Some(ssrc) = ssrc_to_push {
			self.push_event(time, ssrc, evt);
		} else {
			self.push_ssrcless_event(time, user_id.get(), evt);
		}
	}

	pub fn speaking(&mut self, time: Instant, ssrc: u32, speaking: bool) {
		let evt = Event::Speaking(ssrc as u64, speaking);
		self.push_event(time, ssrc, evt);
	}

	pub async fn convert_to_stored(
		&mut self,
		user_data: Arc<UserState>,
		guild_state: Arc<RwLock<GuildState>>,
		ctx: &Context,
	) -> FelyneTrace {
		let final_time = Instant::now();

		let length = final_time
			.checked_duration_since(self.start_time)
			.unwrap_or_default()
			.as_nanos();

		let (guild_id, server_opt) = {
			let lock = guild_state.read().await;
			let guild_id = lock.guild();
			let server_opt = lock.server_opt();

			(guild_id, server_opt)
		};

		// TODO: fix
		let region = None;

		let mut users_to_exclude = HashSet::new();

		for u_id in self.user_to_ssrcs.keys() {
			// NOTE: server opt out is checked at the start to save processing!
			let has_needed_role = if let Some(signed_role) = server_opt.to_role() {
				let role_id = signed_role as u64;

				// Failure to find does NOT constitute consent to measurement.
				// Assume that a lookup error at any stage means you don't have the opt-in role.
				match u_id.to_user(ctx).await {
					Ok(user) => user.has_role(ctx, guild_id, role_id).await.unwrap_or(false),
					_ => false,
				}
			} else {
				// No opt-in system on this server.
				// Rely on pre-check and opt-out.
				true
			};

			if (!has_needed_role) || user_data.is_opted_out(*u_id) {
				users_to_exclude.insert(*u_id);
			}
		}

		let total_user_count = self.user_to_ssrcs.len();

		let server = self.servers.pop_front().map(|(_t, s)| s);

		let (events, user_id_to_opaque) = self.unify_event_streams(&users_to_exclude);

		let optout_users = users_to_exclude
			.iter()
			.filter_map(|user_id| user_id_to_opaque.get(user_id).copied())
			.collect();

		FelyneTrace::Vers2(FelyneTraceV2 {
			events,
			length,
			label: self.label.into(),
			region,
			server,
			region_override: self.region_override.clone(),
			optout_users,
			total_user_count,
			starting_user_count: self.users_at_start,
		})
	}

	pub fn unify_event_streams(
		&mut self,
		forbid: &HashSet<UserId>,
	) -> (Vec<TimedEvent>, HashMap<UserId, u64>) {
		let mut events = Vec::new();
		let mut user_id_to_opaque = HashMap::new();
		let mut ssrc_to_opaque = HashMap::new();

		let unhandled_user_evts: usize = self.user_streams.iter().map(|(_, list)| list.len()).sum();
		let unhandled_events: usize =
			self.rtcps.len() + self.lost_events.len() + unhandled_user_evts;

		for _ in 0..unhandled_events {
			let maybe_found_uid = match self.pull_event() {
				Some(EventSource::Rtcp(evt)) | Some(EventSource::Aux(evt)) => {
					events.push(evt);
					None
				},
				Some(EventSource::User(ssrc, evt)) => {
					let evt_user_id = if let Some(user_id) = self.ssrc_to_user.get(&ssrc) {
						*user_id
					} else {
						// synth a new random userid.
						// this at least allows some level of recovery from
						// a missing map?
						let mut trial = 0u64;

						loop {
							if self.user_to_ssrcs.contains_key(&UserId::new(trial)) {
								trial += 1;
							} else {
								break;
							}
						}

						let u_id = UserId::new(trial);
						self.register_ssrc_userid(ssrc, u_id);
						u_id
					};

					events.push(evt);

					Some(evt_user_id)
				},
				Some(EventSource::Ssrcless(u_id, evt)) => {
					events.push(evt);
					Some(UserId::new(u_id))
				},
				None => {
					eprintln!("Tried to pull extra event!");
					None
				},
			};

			if let Some(u_id) = maybe_found_uid {
				// try to add an opaque mapping.
				let next_opaque = (user_id_to_opaque.len() as u64).min(MISSING_ID as u64);
				user_id_to_opaque.entry(u_id).or_insert(next_opaque);
			}
		}

		// insert ssrc -> opaques.
		for (ssrc, user) in self.ssrc_to_user.iter() {
			if let Some(opaque) = user_id_to_opaque.get(user) {
				ssrc_to_opaque.insert(*ssrc, *opaque);
			}
		}

		// Now put in our own mappings.
		for ssrc in &self.my_ssrcs {
			if let Some(user_id) = self.my_uid {
				user_id_to_opaque.insert(user_id, LISTENER_ID as u64);
			}
			ssrc_to_opaque.insert(*ssrc, LISTENER_ID as u64);
		}

		// after sort, then we need to anonymise each.
		// we do this here to use the *completed* map.

		let mut reduced_events: Vec<TimedEvent> = events
			.into_iter()
			.filter(|(_time, evt)| filter_packet_event(evt, forbid, &self.ssrc_to_user))
			.collect();

		for (_time, evt) in reduced_events.iter_mut() {
			self.anonymise_packet_event(evt, &user_id_to_opaque, &ssrc_to_opaque);
		}

		(reduced_events, user_id_to_opaque)
	}

	fn pull_event(&mut self) -> Option<EventSource> {
		let rtcp_time = self.rtcps.front().map(|(time, _)| *time);
		let user_time_index = self
			.user_streams
			.iter()
			.filter_map(|(ssrc, list)| list.front().map(|(time, _)| (*ssrc, *time)))
			.min_by(|(_, x), (_, y)| x.cmp(y));
		let ssrcless_id_time = self
			.lost_events
			.front()
			.map(|(u_id, (time, _))| (*u_id, *time));

		let server_change_time = self.servers.front().map(|(time, _)| *time);

		let user_time = user_time_index.map(|(_, t)| t);
		let sless_time = ssrcless_id_time.map(|(_, t)| t);

		let best_time = &[rtcp_time, user_time, sless_time, server_change_time]
			.iter()
			.enumerate()
			.filter_map(|(index, maybe_time)| maybe_time.map(|time| (index, time)))
			.min_by(|(_, time), (_, time_2)| time.cmp(time_2));

		match best_time {
			Some((0, _time)) => {
				//rtcp
				self.rtcps
					.pop_front()
					.map(|evt| self.make_event_relative(evt))
					.map(EventSource::Rtcp)
			},

			Some((1, _time)) => {
				// user
				let (ssrc, _) = user_time_index.unwrap();
				self.user_streams
					.get_mut(&ssrc)
					.and_then(|v| v.pop_front())
					.map(|evt| self.make_event_relative(evt))
					.map(|evt| EventSource::User(ssrc, evt))
			},

			Some((2, _time)) => {
				//ssrcless
				let (u_id, _) = ssrcless_id_time.unwrap();

				self.lost_events
					.pop_front()
					.map(|(_, evt)| self.make_event_relative(evt))
					.map(|evt| EventSource::Ssrcless(u_id, evt))
			},

			Some((3, _time)) => {
				//servers
				self.servers
					.pop_front()
					.map(|(t, s)| (t, Event::ChangeServer(s)))
					.map(|evt| self.make_event_relative(evt))
					.map(EventSource::Aux)
			},

			_ => None,
		}
	}

	pub fn make_event_relative(&self, evt: LocalTimedEvent) -> TimedEvent {
		(
			evt.0.saturating_duration_since(self.start_time).as_nanos(),
			match evt.1 {
				Event::Packet {
					sender_id,
					sequence,
					timestamp,
					audio_bytes,
					extension,
				} => {
					let (base_seq, base_ts) = self
						.first_measures
						.get(&(sender_id as u32))
						.unwrap_or(&(0, 0));

					Event::Packet {
						sender_id,
						sequence: sequence.wrapping_sub(*base_seq),
						timestamp: timestamp.wrapping_sub(*base_ts),
						audio_bytes,
						extension,
					}
				},
				e => e,
			},
		)
	}

	fn anonymise_packet_event(
		&mut self,
		evt: &mut Event,
		user_to_opaque: &HashMap<UserId, u64>,
		ssrc_to_opaque: &HashMap<u32, u64>,
	) {
		let mut ssrc_ntp_timestamp_map = HashMap::new();
		let mut ssrc_sr_ntp_rtp_timestamp_map = HashMap::new();

		match evt {
			// These still contain SSRCs.
			Event::Packet {
				ref mut sender_id, ..
			} => {
				*sender_id = *ssrc_to_opaque
					.get(&(*sender_id as u32))
					.unwrap_or(&(MISSING_ID as u64));
			},
			Event::RtcpData(ref mut bytes) => {
				self.sanitise_rtcp(
					bytes,
					ssrc_to_opaque,
					&mut ssrc_ntp_timestamp_map,
					&mut ssrc_sr_ntp_rtp_timestamp_map,
				);
			},
			Event::Speaking(ref mut uid, _) => {
				*uid = *ssrc_to_opaque
					.get(&(*uid as u32))
					.unwrap_or(&(MISSING_ID as u64));
			},

			// These already contain UserIDs
			Event::Connect(ref mut uid) =>
				*uid = *user_to_opaque
					.get(&UserId::new(*uid))
					.unwrap_or(&(MISSING_ID as u64)),
			Event::Disconnect(ref mut uid) =>
				*uid = *user_to_opaque
					.get(&UserId::new(*uid))
					.unwrap_or(&(MISSING_ID as u64)),
			Event::SpeakState(ref mut uid, _) =>
				*uid = *user_to_opaque
					.get(&UserId::new(*uid))
					.unwrap_or(&(MISSING_ID as u64)),
			_ => {},
		}
	}

	pub fn sanitise_rtcp(
		&mut self,
		bytes: &mut Vec<u8>,
		ssrc_to_opaque: &HashMap<u32, u64>,
		ssrc_ntp_timestamp_map: &mut HashMap<u32, u32>,
		ssrc_sr_ntp_rtp_timestamp_map: &mut HashMap<u32, (u64, u32)>,
	) {
		use MutableRtcpPacket as M;
		let mut compound_cursor = 0;
		let pkt_len = bytes.len();

		while compound_cursor < pkt_len {
			if compound_cursor > 0 {
				println!("compound_cursor {:?}", &bytes[compound_cursor..]);
			}
			match demux::demux_mut(&mut bytes[compound_cursor..]) {
				DemuxedMut::Rtcp(M::SenderReport(mut sr)) => {
					let old_ssrc = sr.get_ssrc();
					let opaque = ssrc_to_opaque
						.get(&old_ssrc)
						.copied()
						.unwrap_or(MISSING_ID as u64);
					sr.set_ssrc(opaque as u32);

					compound_cursor += sr.packet_size();
					let n_rx_reports = sr.get_rx_report_count();

					if let Some(mut sender_info) = MutableSenderInfoPacket::new(sr.payload_mut()) {
						let (base_ntp, base_rtp) = ssrc_sr_ntp_rtp_timestamp_map
							.entry(old_ssrc)
							.or_insert_with(|| {
								let ntp = (u64::from(sender_info.get_ntp_timestamp_second()) << 32)
									+ u64::from(sender_info.get_ntp_timestamp_fraction());
								(ntp, sender_info.get_rtp_timestamp())
							});

						let ntp_old = (u64::from(sender_info.get_ntp_timestamp_second()) << 32)
							+ u64::from(sender_info.get_ntp_timestamp_fraction());
						let ntp_new = ntp_old.wrapping_sub(*base_ntp);

						sender_info.set_ntp_timestamp_second((ntp_new >> 32) as u32);
						sender_info.set_ntp_timestamp_fraction(ntp_new as u32);
						sender_info.set_rtp_timestamp(
							sender_info.get_rtp_timestamp().wrapping_sub(*base_rtp),
						);

						let mut blocks = 0;
						let mut cursor = 0;
						while let Some(mut block) =
							MutableReportBlockPacket::new(&mut sender_info.payload_mut()[cursor..])
						{
							self.sanitise_rtcp_report_block(
								&mut block,
								ssrc_to_opaque,
								ssrc_ntp_timestamp_map,
							);

							cursor += block.packet_size();
							blocks += 1;

							if blocks >= n_rx_reports {
								break;
							}
						}

						compound_cursor += cursor;
						compound_cursor += sender_info.packet_size();
					}
				},
				DemuxedMut::Rtcp(M::ReceiverReport(mut rr)) => {
					let opaque = ssrc_to_opaque
						.get(&rr.get_ssrc())
						.copied()
						.unwrap_or(MISSING_ID as u64);
					rr.set_ssrc(opaque as u32);

					let n_rx_reports = rr.get_rx_report_count();
					compound_cursor += rr.packet_size();

					let mut cursor = 0;
					let mut blocks = 0;
					while let Some(mut block) =
						MutableReportBlockPacket::new(&mut rr.payload_mut()[cursor..])
					{
						self.sanitise_rtcp_report_block(
							&mut block,
							ssrc_to_opaque,
							ssrc_ntp_timestamp_map,
						);

						cursor += block.packet_size();

						blocks += 1;

						if blocks >= n_rx_reports {
							break;
						}
					}

					compound_cursor += cursor;
				},
				_ => {
					// This is necessary to prevent an infinite loop from bringing us down!
					break;
				},
			}
		}
	}

	pub fn sanitise_rtcp_report_block(
		&mut self,
		block: &mut MutableReportBlockPacket,
		ssrc_to_opaque: &HashMap<u32, u64>,
		ssrc_ntp_timestamp_map: &mut HashMap<u32, u32>,
	) {
		let old_ssrc = block.get_ssrc();
		let opaque = ssrc_to_opaque
			.get(&old_ssrc)
			.copied()
			.unwrap_or(MISSING_ID as u64);
		block.set_ssrc(opaque as u32);

		let seen_ts = block.get_last_sr_timestamp();
		if seen_ts > 0 {
			let base_ts = ssrc_ntp_timestamp_map.entry(old_ssrc).or_insert(seen_ts);

			block.set_last_sr_timestamp(seen_ts.wrapping_sub(*base_ts));
		}

		let seen_seq = block.get_sequence();
		let (base_seq, _) = self
			.first_measures
			.entry(old_ssrc)
			.or_insert_with(|| (seen_seq, 0));

		block.set_sequence(seen_seq.wrapping_sub(*base_seq));
	}
}

enum EventSource {
	Rtcp(TimedEvent),
	User(u32, TimedEvent),
	Ssrcless(u64, TimedEvent),
	Aux(TimedEvent),
}

fn filter_packet_event(
	evt: &Event,
	forbid: &HashSet<UserId>,
	to_user: &HashMap<u32, UserId>,
) -> bool {
	match evt {
		// These ones are derived from per-packet measures.
		Event::Packet { sender_id, .. } => to_user
			.get(&(*sender_id as u32))
			.map(|id| !forbid.contains(id))
			.unwrap_or(true),
		Event::Speaking(ssrc, _) => to_user
			.get(&(*ssrc as u32))
			.map(|id| !forbid.contains(id))
			.unwrap_or(true),
		_ => true,
	}
}
