//! One authenticated Discord session: REST through `discord-api`, live events through
//! `discord-gateway`, both feeding client-core `Envelope`s back to the UI thread.
//!
//! This is the slim counterpart of the desktop app's `connection.rs`. It runs the same
//! transport crates with the same secret handling, but only the command routing the
//! messaging views need: history reads replace each other, and every write goes through
//! one ordered queue so sends land in the order the user typed them.

use client_core::{
	COMMAND_SLOTS, Command, Envelope, Event,
	auth::{Failure, SessionSecret},
};
use discord_api::DiscordApi;
use std::sync::{Arc, mpsc as std_mpsc};
use tokio::{
	runtime::Handle,
	sync::{mpsc, watch},
	task::JoinHandle,
};

/// Calls back into the UI event loop after events are queued.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Session {
	commands: mpsc::Sender<Command>,
	events: std_mpsc::Receiver<Envelope>,
	terminal: watch::Receiver<Option<Failure>>,
	task: JoinHandle<()>,
}

impl Drop for Session {
	fn drop(&mut self) {
		// Aborting the supervisor drops the gateway, writer and history tasks it owns.
		self.task.abort();
	}
}

struct AbortOnDrop(JoinHandle<()>);
impl Drop for AbortOnDrop {
	fn drop(&mut self) {
		self.0.abort();
	}
}

impl Session {
	pub fn start(
		runtime: &Handle,
		secret: Arc<SessionSecret>,
		generation: u64,
		expected_user: Option<model::Id>,
		wake: Wake,
	) -> Self {
		let (commands, receive) = mpsc::channel(COMMAND_SLOTS);
		let (send, events) = std_mpsc::channel();
		let (finished, terminal) = watch::channel(None);
		let emit_wake = wake.clone();
		let emit = Arc::new(move |event: Event| -> Result<(), Failure> {
			send.send(Envelope { generation, event })
				.map_err(|_| Failure::Network)?;
			emit_wake();
			Ok(())
		});
		let task = runtime.spawn(async move {
			let result = run(secret, expected_user, receive, emit).await;
			if let Err(failure) = result {
				let _ = finished.send(Some(failure));
			}
			wake();
		});
		Self {
			commands,
			events,
			terminal,
			task,
		}
	}

	/// Queues a command. A full or closed queue hands it back so the caller can report it.
	pub fn send(&self, command: Command) -> Result<(), Box<Command>> {
		self.commands
			.try_send(command)
			.map_err(|error| match error {
				mpsc::error::TrySendError::Full(command)
				| mpsc::error::TrySendError::Closed(command) => Box::new(command),
			})
	}

	pub fn drain(&self) -> Vec<Envelope> {
		self.events.try_iter().collect()
	}

	/// The failure that stopped the session, once it stopped.
	pub fn terminal(&self) -> Option<Failure> {
		*self.terminal.borrow()
	}
}

type Emit = Arc<dyn Fn(Event) -> Result<(), Failure> + Send + Sync>;

/// Failures in a write result that end the whole session (expired or replaced token).
fn ends_session(event: &Event) -> Option<Failure> {
	let failure = match event {
		Event::Failure(failure)
		| Event::SendResult {
			result: Err(failure),
			..
		} => *failure,
		Event::TypingSent(Err(failure)) if *failure != Failure::Capacity => *failure,
		_ => return None,
	};
	failure.ends_session().then_some(failure)
}

async fn run(
	secret: Arc<SessionSecret>,
	expected_user: Option<model::Id>,
	mut commands: mpsc::Receiver<Command>,
	emit: Emit,
) -> Result<(), Failure> {
	let (_proxy, proxy) = watch::channel(None);
	let api = DiscordApi::with_proxy(secret.clone(), proxy)?;
	// Discovery does not depend on the account check, so they share one round trip.
	let (user, gateway) = tokio::join!(api.current_user(), api.gateway_url());
	let user = user?;
	if expected_user.is_some_and(|id| id != user.id) {
		return Err(Failure::InvalidCredential);
	}
	let gateway = gateway?;
	let api = Arc::new(api);

	// The first task to stop records why, so the UI reports the real cause.
	let stopped = Arc::new(std::sync::Mutex::new(None::<Failure>));
	let record = |slot: &std::sync::Mutex<Option<Failure>>, failure: Failure| {
		if let Ok(mut slot) = slot.lock() {
			slot.get_or_insert(failure);
		}
	};
	let (_members, member_subscriptions) = watch::channel(None);
	let gateway_stopped = stopped.clone();
	let gateway_emit = emit.clone();
	let gateway_api = api.clone();
	let stop_api = api.clone();
	let user_id = user.id;
	let mut gateway_task = AbortOnDrop(tokio::spawn(async move {
		let error = discord_gateway::run(secret, gateway, member_subscriptions, move |event| {
			if let Some((ready_user, _, _)) = event.ready_navigation()
				&& ready_user.id != user_id
			{
				return Err(Failure::InvalidCredential);
			}
			if matches!(&event, Event::Disconnected | Event::Resync) {
				gateway_api.interaction_session(None)?;
			}
			if let Event::Interaction(client_core::interactions::Event::Session(session)) = event {
				return gateway_api.interaction_session(Some(session));
			}
			gateway_emit(event)
		})
		.await
		.err()
		.unwrap_or(Failure::Network)
		.protocol_at("Gateway connection: unsupported handshake or event");
		stop_api.stop();
		record(&gateway_stopped, error);
	}));

	// Writes keep their order; a session-ending result stops everything.
	let (write_send, mut write_receive) = mpsc::channel::<Command>(COMMAND_SLOTS);
	let write_api = api.clone();
	let write_emit = emit.clone();
	let writer_stopped = stopped.clone();
	let mut writer = AbortOnDrop(tokio::spawn(async move {
		while let Some(command) = write_receive.recv().await {
			let throttle_emit = write_emit.clone();
			let event = write_api
				.execute_write(command, |nonce| {
					let _ = throttle_emit(Event::SendThrottled {
						nonce: nonce.to_owned(),
					});
				})
				.await;
			let stop = ends_session(&event);
			if let Err(failure) = write_emit(event) {
				record(&writer_stopped, failure);
				write_api.stop();
				break;
			}
			if let Some(failure) = stop {
				record(&writer_stopped, failure);
				write_api.stop();
				break;
			}
		}
	}));

	let mut history: Option<AbortOnDrop> = None;
	loop {
		tokio::select! {
			_ = &mut gateway_task.0 => break,
			_ = &mut writer.0 => break,
			command = commands.recv() => {
				let Some(command) = command else { break };
				match command {
					// A newer history read replaces the one in flight, like switching channels.
					Command::History { .. } => {
						let api = api.clone();
						let emit = emit.clone();
						history = Some(AbortOnDrop(tokio::spawn(async move {
							let event = api.execute(command).await;
							let stop = ends_session(&event);
							if emit(event).is_err() || stop.is_some() {
								api.stop();
							}
						})));
					}
					Command::Typing { .. } => {
						// Optional; never block or fail the session on a full queue.
						let _ = write_send.try_send(command);
					}
					command => {
						if write_send.send(command).await.is_err() {
							break;
						}
					}
				}
			}
		}
	}
	drop(history);
	api.stop();
	let failure = stopped.lock().ok().and_then(|slot| *slot);
	Err(failure.unwrap_or(Failure::Network))
}
