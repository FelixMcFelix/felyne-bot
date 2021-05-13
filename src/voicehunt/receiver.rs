use super::live::LiveTrace;
use crate::{
	config::{GatherMode, OptInOut},
	constants::TRACE_DIR,
	guild::GuildState,
	user::UserState,
};
use felyne_trace::FelyneTrace;
use flume::{Receiver, Sender, TryRecvError};
use serenity::{
	async_trait,
	client::Context,
	model::prelude::{Channel, ChannelId, GuildId, UserId},
};
use songbird::{
	events::{CoreEvent, Event, EventContext, EventHandler},
	Call,
};
use std::{
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::{Instant, SystemTime},
};
use tokio::{fs::File, sync::RwLock};
use tracing::error;

#[derive(Clone)]
pub struct VoiceHuntReceiver {
	trace: Arc<RwLock<Option<LiveTrace>>>,
	rx: Receiver<ReceiverSignal>,
	tx: Sender<ReceiverSignal>,

	never_act: Arc<AtomicBool>,
	do_nothing: Arc<AtomicBool>,
	gather_mode: GatherMode,
	user_states: Arc<UserState>,
	guild_id: GuildId,
	guild_state: Arc<RwLock<GuildState>>,

	ctx: Context,
}

impl VoiceHuntReceiver {
	pub async fn new(
		opt_in: OptInOut,
		gather_mode: GatherMode,
		user_states: Arc<UserState>,
		guild_id: GuildId,
		channel_id: ChannelId,
		guild_state: Arc<RwLock<GuildState>>,
		making_noise: bool,
		initial_user_count: usize,
		ctx: Context,
	) -> Self {
		let (tx, rx) = flume::bounded(1);

		let never_act = opt_in.opted_out();
		let prevent = match gather_mode {
			GatherMode::AlwaysGather => false,
			GatherMode::GatherActive => !making_noise,
		};
		let do_nothing = Arc::new((never_act || prevent).into());

		let label = {
			let lock = guild_state.read().await;
			lock.label()
		};

		let user_id = ctx.http.get_current_user().await.ok().map(|cu| cu.id);
		let rtc_region = channel_id
			.to_channel_cached(&ctx)
			.await
			.and_then(Channel::guild)
			.and_then(|gc| gc.rtc_region);

		Self {
			trace: Arc::new(RwLock::new(Some(LiveTrace::new(
				Instant::now(),
				label,
				initial_user_count,
				rtc_region,
				user_id,
			)))),
			rx,
			tx,

			never_act: Arc::new(never_act.into()),
			do_nothing,
			gather_mode,
			user_states,
			guild_id,
			guild_state,
			ctx,
		}
	}

	fn handle_possible_cancel(&self) -> bool {
		match self.rx.try_recv() {
			Ok(ReceiverSignal::Poison) | Err(TryRecvError::Disconnected) => {
				let _ = self.tx.send(ReceiverSignal::Poison);
				true
			},
			Ok(ReceiverSignal::Active) => {
				let never_act = self.never_act.load(Ordering::Relaxed);

				self.do_nothing.store(never_act, Ordering::Relaxed);

				false
			},
			Ok(ReceiverSignal::Inactive) => {
				let never_act = self.never_act.load(Ordering::Relaxed);
				let prevent = !matches!(self.gather_mode, GatherMode::AlwaysGather);

				self.do_nothing
					.store(never_act || prevent, Ordering::Relaxed);

				false
			},
			Err(TryRecvError::Empty) => false,
		}
	}
}

#[async_trait]
impl EventHandler for VoiceHuntReceiver {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
		let time = Instant::now();

		if self.handle_possible_cancel() {
			Some(Event::Cancel)
		} else {
			if self.do_nothing.load(Ordering::Relaxed) {
				return None;
			}

			match ctx {
				EventContext::SpeakingStateUpdate(s) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.speaking_state(time, s);
					}

					None
				},
				EventContext::VoicePacket(vp) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.packet(time, vp.packet, vp.payload_offset, vp.payload_end_pad);
					}

					None
				},
				EventContext::RtcpPacket(rp) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.rtcp(time, rp.packet, rp.payload_offset, rp.payload_end_pad);
					}

					None
				},
				EventContext::ClientConnect(s) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.client_connect(time, s.audio_ssrc, UserId(s.user_id.0));
					}

					None
				},
				EventContext::ClientDisconnect(s) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.client_disconnect(time, UserId(s.user_id.0));
					}

					None
				},
				EventContext::SpeakingUpdate(su) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.speaking(time, su.ssrc, su.speaking);
					}

					None
				},
				EventContext::DriverConnect(d) | EventContext::DriverReconnect(d) => {
					if let Some(trace) = &mut *self.trace.write().await {
						trace.add_my_ssrc(d.ssrc);

						trace.change_server(time, d.server.to_string())
					}

					None
				},
				_ => None,
			}
		}
	}
}

impl Drop for VoiceHuntReceiver {
	fn drop(&mut self) {
		// Easier to do than having a synchro mechanism.
		// Joining a new channel will cause the old receiver to be dropped.
		// Similarly, leaving completely will do the same...

		finalise_audio_session(
			self.trace.clone(),
			self.user_states.clone(),
			self.guild_state.clone(),
			self.ctx.clone(),
		);
	}
}

fn finalise_audio_session(
	trace: Arc<RwLock<Option<LiveTrace>>>,
	user_data: Arc<UserState>,
	guild_state: Arc<RwLock<GuildState>>,
	ctx: Context,
) {
	tokio::spawn(async move {
		let anonymised: Option<FelyneTrace> = {
			let mut lock = trace.write().await;

			if let Some(mut trace) = lock.take() {
				Some(trace.convert_to_stored(user_data, guild_state, &ctx).await)
			} else {
				None
			}
		};

		let time_name = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
		if let Ok(t) = time_name {
			let fname = format!("{}/{}.bc", TRACE_DIR, t.as_micros());
			if let Some(trace) = &anonymised {
				let out = File::create(&fname).await;

				if let Ok(out) = out {
					if let Err(e) = felyne_trace::write_async(out, &trace).await {
						error!("Failed to write trace: {:?}", e);
					}
				}
			}
		} else {
			error!("Apparently times are hard.");
		}
	});
}

pub enum ReceiverSignal {
	Active,
	Inactive,
	Poison,
}

pub async fn listen_in(
	handler: &mut Call,
	opt_in: OptInOut,
	gather_mode: GatherMode,
	user_states: Arc<UserState>,
	guild_state: Arc<RwLock<GuildState>>,
	guild_id: GuildId,
	channel_id: ChannelId,
	making_noise: bool,
	initial_user_count: usize,
	ctx: Context,
) -> Option<Sender<ReceiverSignal>> {
	if opt_in.opted_out() {
		None
	} else {
		let vhr = VoiceHuntReceiver::new(
			opt_in,
			gather_mode,
			user_states,
			guild_id,
			channel_id,
			guild_state,
			making_noise,
			initial_user_count,
			ctx,
		)
		.await;
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

		let n_vhr = vhr.clone();
		handler.add_global_event(CoreEvent::DriverConnect.into(), n_vhr);

		let n_vhr = vhr.clone();
		handler.add_global_event(CoreEvent::DriverReconnect.into(), n_vhr);

		handler.add_global_event(CoreEvent::SpeakingUpdate.into(), vhr);

		Some(out_tx)
	}
}
