use std::{
	cmp::Eq,
	collections::HashMap,
	hash::Hash,
	time::{
		Duration,
		Instant,
	},
};

#[derive(Copy, Clone, Debug)]
pub struct Transition<State: Copy> {
	destination: State,
	priority: usize,
	cooldown: Option<Cooldown>,
	last_used: Option<Instant>,
}

#[derive(Copy, Clone, Debug)]
pub struct Cooldown {
	duration: Duration,
	refresh: bool,
	start_used: bool,
}

pub struct TimedMachine<State: Hash + Eq + Copy, Alphabet: Hash + Eq> {
	state: State,
	transitions: HashMap<State, HashMap<Alphabet, Vec<Transition<State>>>>,
}

impl<State: Hash + Eq + Copy, Alphabet: Hash + Eq> TimedMachine<State, Alphabet> {
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

	pub fn add_transition(&mut self, from: State, to: State, on: Alphabet) -> &mut Self {
		self.add_priority_transition(from, to, on, 0, None)
	}

	pub fn add_priority_transition(&mut self, from: State, to: State, on: Alphabet, priority: usize, cooldown: Option<Cooldown>) -> &mut Self {
		// higher prio == comes first.
		let alpha_set = self.transitions
			.entry(from)
			.or_insert(HashMap::new());
		let tx_list = alpha_set
			.entry(on)
			.or_insert(Vec::new());

		// existing transitions with same priority are overridden.
		let pos = tx_list.binary_search_by_key(&priority, |el| el.priority);
		let tx = Transition {
			destination: to,
			priority,
			cooldown,
			last_used: cooldown.and_then(|cd| if cd.start_used { Some(Instant::now()) } else { None }),
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
					if let Some(cd) = tx.cooldown {
						if cd.refresh {
							tx.last_used = if cd.start_used { Some(Instant::now()) } else { None };
						}
					}
				}
			}
		}
        self
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
        machine.add_transition(TestState::A, TestState::B, TestAlpha::A);
        machine.add_priority_transition(TestState::A, TestState::C, TestAlpha::A, 1, None);

        assert_eq!(machine.advance(TestAlpha::A), Some(TestState::C));
    }

    #[test]
    fn test_tx_cooldown() {
        let mut machine = TimedMachine::new(TestState::A);
        machine.add_transition(TestState::A, TestState::B, TestAlpha::A);

        let cd = Cooldown {
            duration: Duration::from_secs(200),
            refresh: false,
            start_used: false,
        };
        machine.add_priority_transition(TestState::A, TestState::C, TestAlpha::A, 1, Some(cd));
        machine.add_transition(TestState::C, TestState::A, TestAlpha::A);

        machine.advance(TestAlpha::A);
        machine.advance(TestAlpha::A);
        assert_eq!(machine.advance(TestAlpha::A), Some(TestState::B));
    }
}
