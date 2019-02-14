use std::{
	cmp::{
		Eq,
		Ord,
		Ordering,
		PartialEq,
		PartialOrd,
	},
	collections::HashMap,
	hash::Hash,
	time::{
		Duration,
		Instant,
	},
}

#[derive(Eq)]
pub struct Transition<State> {
	destination: State,
	priority: usize,
	cooldown: Option<Cooldown>,
	last_used: Option<Instant>,
}

impl<State> Ord for Transition<State> {
	fn cmp(&self, other: &Self) -> Ordering {
		self.priority.cmp(other.priority)
	}
}

impl<State> PartialEq for Transition<State> {
	fn eq(&self, other: &Self) -> bool {
		self.priority == other.priority
	}
}

impl<State> PartialOrd for Transition<State> {
	fn partial_cmp(&self, other: &Self) -> Ordering {
		Some(self.cmp(other))
	}
}

pub struct Cooldown {
	duration: Duration,
	refresh: bool,
	start_used: bool,
}

pub struct TimedMachine<State: Hash + Eq, Alphabet: Hash + Eq> {
	state: State,
	transitions: HashMap<State, HashMap<Alphabet, Vec<Transition>>>,
}

impl<State, Alphabet> TimedMachine<State, Alphabet> {
	pub new(start: State) -> Self {
		Self {
			state: start,
			transitions: HashMap::new(),
		}
	}

	pub advance(&mut self, token: Alphabet) -> Option<State> {
		let mut tx = self.transitions.get(self.state)
			.map(move |token_map| token_map.get(token))
			.and_then(|tx_list|
				tx_list.iter_mut()
					.rfind(|tx| match (tx.cooldown, tx.last_used) {
						(Some(cd), Some(time)) => time.elapsed() > cd.duration,
						_ => true,
					})
			);

		if let Some(mut tx) = tx {
			tx.last_used = Some(Instant::now());
			self.state = tx.destination;
			Some(self.state)
		} else {
			None
		}
	}

	pub add_transition(&mut self, from: State, to: State, on: Alphabet) -> &mut Self {
		self.add_prio_transition(from, to, on, 0, None)
	}

	pub add_priority_transition(&mut self, from: State, to: State, on: Alphabet, priority: usize, cooldown: Option<Cooldown>) {
		// higher prio == comes first.
		let mut alpha_set = self.transitions
			.entry(from)
			.or_insert(HashMap::new());
		let mut tx_list = alpha_set
			.entry(on)
			.or_insert(Vec::new());

		// existing transitions with same priority are overridden.
		let pos = *tx_list.binary_search(&priority);
		let tx = Transition {
			destination: to,
			priority,
			cooldown,
			last_used: if cooldown.start_used { Some(Instant::now()) } else { None },
		};

		match pos {
			Ok(exist_pos) => tx_list[exist_pos] = tx,
			Err(insert_pos) => tx_list.insert(insert_pos, tx),
		}

		self
	}

	pub refresh(&mut self) {
		unimplemented!()
	}
}
