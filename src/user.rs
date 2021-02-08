use crate::dbs::*;
use dashmap::DashSet;
use serenity::{model::prelude::UserId, prelude::*};
use std::sync::Arc;

pub struct UserStateKey;

impl TypeMapKey for UserStateKey {
	type Value = Arc<UserState>;
}

pub struct UserState {
	db: Arc<FelyneDb>,
	optouts: DashSet<UserId>,
}

impl UserState {
	pub async fn new(db: Arc<FelyneDb>) -> Self {
		let users = db.select_optout_users().await;
		let optouts = DashSet::new();

		if let Ok(users) = users {
			for user in users {
				optouts.insert(user);
			}
		}

		Self { db, optouts }
	}

	pub async fn optout(&self, u_id: UserId) {
		self.db.upsert_optout(u_id).await;
		self.optouts.insert(u_id);
	}

	pub async fn optin(&self, u_id: UserId) {
		self.db.delete_optout(u_id).await;
		self.optouts.remove(&u_id);
	}

	#[inline]
	pub fn is_opted_out(&self, u_id: UserId) -> bool {
		self.optouts.contains(&u_id)
	}
}
