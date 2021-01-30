use crate::dbs::*;
use dashmap::DashSet;
use serenity::{model::prelude::UserId, prelude::*};
use std::sync::Arc;
use tokio_postgres::Client;

pub struct UserStateKey;

impl TypeMapKey for UserStateKey {
	type Value = Arc<UserState>;
}

pub struct UserState {
	db: Arc<Client>,
	optouts: DashSet<UserId>,
}

impl UserState {
	pub async fn new(db: Arc<Client>) -> Self {
		let users = select_optout_users(&db).await;
		let optouts = DashSet::new();

		if let Ok(users) = users {
			for user in users {
				optouts.insert(user);
			}
		}

		Self { db, optouts }
	}

	pub async fn optout(&self, u_id: UserId) {
		upsert_optout(&self.db, u_id).await;
		self.optouts.insert(u_id);
	}

	pub async fn optin(&self, u_id: UserId) {
		delete_optout(&self.db, u_id).await;
		self.optouts.remove(&u_id);
	}

	#[inline]
	pub fn is_opted_out(&self, u_id: UserId) -> bool {
		self.optouts.contains(&u_id)
	}
}
