use rand::{
	distributions::*,
};
use std::{
	cmp::Eq,
	collections::HashMap,
	hash::Hash,
	time::{
		Duration,
		Instant,
	},
};

#[derive(Clone, Debug)]
pub enum DurationSource {
	Exact(Duration),
	Uniform(Uniform<Duration>),
}

impl DurationSource {
	pub fn draw(&self) -> Duration {
		use DurationSource::*;

		match self {
			Exact(d) => *d,
			Uniform(d) => d.sample(&mut rand::thread_rng()),
		}
	}
}

impl From<Duration> for DurationSource {
	fn from(d: Duration) -> Self {
		DurationSource::Exact(d)
	}
}

impl From<Uniform<Duration>> for DurationSource {
	fn from(d: Uniform<Duration>) -> Self {
		DurationSource::Uniform(d)
	}
}

#[derive(Clone, Debug)]
pub struct Transition<State: Copy> {
	destination: State,
	priority: usize,

	cooldown_data: Option<Cooldown>,
	cooldown: Option<Duration>,
	last_used: Instant,
}

impl<State: Copy> Transition<State> {
	fn cooldown(&mut self, data: Option<Cooldown>) {
		self.last_used = Instant::now();
		self.cooldown = data.as_ref()
			.or_else(|_| self.cooldown_data.as_ref())
			.map(Cooldown::draw);
	}
}

#[derive(Clone, Debug)]
pub struct Cooldown {
	duration: DurationSource,
	refresh: bool,
	start_used: bool,
}

impl Cooldown {
	#[inline]
	pub fn draw(&self) -> Duration {
		self.duration.draw()
	}

	pub fn new(duration: DurationSource, refresh: bool, start_used: bool) -> Self {
		Self {
			duration,
			refresh,
			start_used,
		}
	}

	fn cooldown()
}

pub struct TimedMachine<State: Hash + Eq + Copy, Alphabet: Hash + Eq> {
	state: State,
	transitions: HashMap<State, HashMap<Alphabet, Vec<Transition<State>>>>,
}

impl<State: Hash + Eq + Copy, Alphabet: Hash + Eq + Copy> TimedMachine<State, Alphabet> {
	pub fn new(start: State) -> Self {
		Self {
			state: start,
			transitions: HashMap::new(),
		}
	}

	pub fn advance(&mut self, token: Alphabet) -> Option<State> {
		let tx = self.transitions.get_mut(&self.state)
			.and_then(move |token_map| token_map.get_mut(&token))
			.and_then(|tx_list|
				tx_list.iter_mut()
					.rfind(|tx| match tx.cooldown {
						Some(cd) => tx.last_used.elapsed() > cd,
						_ => true,
					})
			);

		if let Some(mut tx) = tx {
			tx.cooldown()

			self.state = tx.destination;
			Some(self.state)
		} else {
			None
		}
	}

	pub fn cause_cooldown(&mut self, from: State, to: State, on: Alphabet, prio: usize, data: Option<Cooldown>) {
		let tx = self.transitions.get_mut(&from)
			.and_then(move |token_map| token_map.get_mut(&on))
			.and_then(|tx_list|
				tx_list.iter_mut()
					.for_each(|tx| if tx.priority == prio && tx.destination == to {
						tx.cooldown(data);
					})
			);
	}

	pub fn register_state(&mut self, state: State) -> &mut Self {
		let alpha_set = self.transitions
			.entry(state)
			.or_insert(HashMap::new());
		self
	}

	pub fn all_transition(&mut self, to: State, on: Alphabet) -> &mut Self {
		let starts: Vec<State> = self.transitions.keys().map(|x| *x).collect();
		for start in starts {
			self.add_transition(start, to, on);
		}
		self
	}

	pub fn add_transition(&mut self, from: State, to: State, on: Alphabet) -> &mut Self {
		self.add_priority_transition(from, to, on, 0, None)
	}

	pub fn add_priority_transition(
			&mut self,
			from: State, to: State, on: Alphabet,
			priority: usize, cooldown_data: Option<Cooldown>
			) -> &mut Self {
		let alpha_set = self.transitions
			.entry(from)
			.or_insert(HashMap::new());
		let tx_list = alpha_set
			.entry(on)
			.or_insert(Vec::new());

		// existing transitions with same priority are overridden.
		let pos = tx_list.binary_search_by_key(&priority, |el| el.priority);
		let cooldown = cooldown_data.as_ref()
			.and_then(|cd|
				if cd.start_used {
					Some(cd.draw())
				} else {
					None
				});
		let tx = Transition {
			destination: to,
			priority,

			last_used: Instant::now(),
			cooldown,
			cooldown_data,
		};

		match pos {
			Ok(exist_pos) => tx_list[exist_pos] = tx,
			Err(insert_pos) => tx_list.insert(insert_pos, tx),
		}

		self
	}

	pub fn refresh(&mut self) -> &mut Self{
		for (_start, token_map) in self.transitions.iter_mut() {
			for (_token, tx_list) in token_map.iter_mut() {
				for tx in tx_list.iter_mut() {
					if let Some(cd) = &tx.cooldown_data {
						if cd.refresh {
							tx.cooldown = None;
						}
					}
				}
			}
		}
		self
	}

	pub fn state(&self) -> State {
		self.state
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
	enum TestState {
		A, B, C,
	}

	#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
	enum TestAlpha {
		A, B, C,
	}

	#[test]
	fn test_tx_basic() {
		let mut machine = TimedMachine::new(TestState::A);
		machine.add_transition(TestState::A, TestState::B, TestAlpha::A);

		assert_eq!(machine.advance(TestAlpha::A), Some(TestState::B));
	}

	#[test]
	fn test_no_tx_basic() {
		let mut machine = TimedMachine::new(TestState::A);
		machine.add_transition(TestState::A, TestState::B, TestAlpha::A);

		assert_eq!(machine.advance(TestAlpha::B), None);
	}

	#[test]
	fn test_tx_self() {
		let mut machine = TimedMachine::new(TestState::A);
		machine.add_transition(TestState::A, TestState::A, TestAlpha::A);

		assert_eq!(machine.advance(TestAlpha::A), Some(TestState::A));
	}

	#[test]
	fn test_tx_multi() {
		let mut machine = TimedMachine::new(TestState::A);
		machine.add_transition(TestState::A, TestState::A, TestAlpha::A)
			.add_transition(TestState::A, TestState::B, TestAlpha::B)
			.add_transition(TestState::A, TestState::C, TestAlpha::C);

		assert_eq!(machine.advance(TestAlpha::C), Some(TestState::C));
	}


	#[test]
	fn test_tx_priority() {
		let mut machine = TimedMachine::new(TestState::A);
		machine.add_transition(TestState::A, TestState::B, TestAlpha::A)
			.add_priority_transition(TestState::A, TestState::C, TestAlpha::A, 1, None);

		assert_eq!(machine.advance(TestAlpha::A), Some(TestState::C));
	}

	#[test]
	fn test_tx_cooldown() {
		let mut machine = TimedMachine::new(TestState::A);
		let cd = Cooldown::new(Duration::from_secs(200).into(), false, false);

		machine.add_transition(TestState::A, TestState::B, TestAlpha::A)
			.add_priority_transition(TestState::A, TestState::C, TestAlpha::A, 1, Some(cd))
			.add_transition(TestState::C, TestState::A, TestAlpha::A);

		machine.advance(TestAlpha::A);
		machine.advance(TestAlpha::A);
		assert_eq!(machine.advance(TestAlpha::A), Some(TestState::B));
	}

	#[test]
	fn test_tx_prob_cooldown() {
		let mut machine = TimedMachine::new(TestState::A);
		let cd = Cooldown::new(
			Uniform::new(Duration::from_secs(100),Duration::from_secs(200)).into(),
			false,
			false);

		machine.add_transition(TestState::A, TestState::B, TestAlpha::A)
			.add_priority_transition(TestState::A, TestState::C, TestAlpha::A, 1, Some(cd))
			.add_transition(TestState::C, TestState::A, TestAlpha::A);

		machine.advance(TestAlpha::A);
		machine.advance(TestAlpha::A);
		assert_eq!(machine.advance(TestAlpha::A), Some(TestState::B));
	}
}
