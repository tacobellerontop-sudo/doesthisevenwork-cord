//! Maps client-core's `State` onto the flat rows the Slint views render.
//!
//! Everything here is pure: given the state and a little view-local navigation, it returns
//! row vectors. That keeps the toolkit binding in `main.rs` thin and lets tests cover the
//! ordering, grouping and naming rules without a window.

use crate::clock::Clock;
use crate::markdown::{Piece, to_markdown};
use client_core::State;
use model::{Channel, Delivery, Id, Message, User};
use std::collections::BTreeSet;

/// Channel kinds as the Slint `ChannelItem.kind` codes them.
pub mod kind {
	pub const TEXT: i32 = 0;
	pub const VOICE: i32 = 1;
	pub const CATEGORY: i32 = 2;
	pub const ANNOUNCEMENT: i32 = 3;
	pub const FORUM: i32 = 4;
	pub const DIRECT: i32 = 5;
	pub const GROUP: i32 = 6;
	pub const THREAD: i32 = 7;
}

/// Discord channel type → UI kind; `None` for types the sidebar does not list.
pub fn channel_kind(discord: u8) -> Option<i32> {
	Some(match discord {
		0 => kind::TEXT,
		2 | 13 => kind::VOICE,
		4 => kind::CATEGORY,
		5 => kind::ANNOUNCEMENT,
		15 | 16 => kind::FORUM,
		1 => kind::DIRECT,
		3 => kind::GROUP,
		10..=12 => kind::THREAD,
		_ => return None,
	})
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuildRow {
	pub id: Id,
	pub name: String,
	pub initials: String,
	pub color: u32,
	pub selected: bool,
	pub unread: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelRow {
	pub id: Id,
	pub name: String,
	pub kind: i32,
	pub selected: bool,
	pub unread: bool,
	pub collapsed: bool,
	pub subtitle: String,
	pub initials: String,
	pub color: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MessageRow {
	pub id: String,
	pub author: String,
	pub initials: String,
	pub avatar_color: u32,
	pub time: String,
	pub short_time: String,
	/// Plain text with references resolved, for accessibility.
	pub content: String,
	/// The same message as Slint styled-text markdown.
	pub markdown: String,
	pub grouped: bool,
	/// 0 delivered, 1 sending, 2 failed, 3 unknown outcome.
	pub delivery: i32,
	pub edited: bool,
	pub divider: String,
	pub reply: String,
	pub system: bool,
	pub mentions_me: bool,
}

/// Discord's default avatar palette, used for initials avatars.
const AVATAR_COLORS: [u32; 6] = [0x5865f2, 0x757e8a, 0x3ba55c, 0xfaa61a, 0xed4245, 0xeb459e];

pub fn avatar_color(id: Id) -> u32 {
	// Snowflakes share their high (time) bits, so mix in the low ones too.
	let mixed = (id.0 >> 22) ^ id.0;
	AVATAR_COLORS[(mixed % AVATAR_COLORS.len() as u64) as usize]
}

/// Message text split around Discord's inline references, resolved for reading:
/// `<@id>` → `@name`, `<#id>` → `#channel`, `<@&id>` → `@role`, `<:name:id>` → `:name:`,
/// `<t:…>` → a local date.
pub fn pieces<'a>(state: &State, message: &'a Message, clock: &Clock) -> Vec<Piece<'a>> {
	let text = message.content.as_str();
	let guild = state.channel(message.channel).and_then(|c| c.guild);
	let mut out = Vec::new();
	let mut rest = text;
	let mut plain_start = 0;
	let mut offset = 0;
	while let Some(found) = rest.find('<') {
		let at = offset + found;
		let tail = &text[at..];
		let resolved = if let Some((id, used)) = model::user_mention_prefix(tail) {
			let name = message
				.mentions
				.iter()
				.find(|user| user.id == id)
				.map(|user| state.user_display_name(user).to_owned())
				.or_else(|| {
					state
						.timeline
						.iter()
						.find(|m| m.author.id == id)
						.map(|m| author_name(state, m).to_owned())
				})
				.unwrap_or_else(|| "unknown-user".to_owned());
			Some((Piece::Mention(format!("@{name}")), used))
		} else if let Some((id, used)) = model::role_mention_prefix(tail) {
			let name = guild
				.and_then(|guild| state.guild_roles(guild))
				.and_then(|roles| roles.iter().find(|role| role.id == id))
				.map_or("unknown-role", |role| role.name.as_str());
			Some((Piece::Mention(format!("@{name}")), used))
		} else if let Some((id, used)) = model::channel_mention_prefix(tail) {
			let name = state.channel(id).map_or("unknown", |c| c.name.as_str());
			Some((Piece::Mention(format!("#{name}")), used))
		} else if let Some((seconds, _, used)) = model::timestamp_prefix(tail) {
			Some((
				Piece::Plain(clock.header_time(seconds.saturating_mul(1000))),
				used,
			))
		} else {
			custom_emoji_prefix(tail).map(|(name, used)| (Piece::Plain(format!(":{name}:")), used))
		};
		match resolved {
			Some((piece, used)) => {
				if plain_start < at {
					out.push(Piece::Text(&text[plain_start..at]));
				}
				out.push(piece);
				plain_start = at + used;
				offset = plain_start;
				rest = &text[plain_start..];
			}
			None => {
				offset = at + 1;
				rest = &text[offset..];
			}
		}
	}
	if plain_start < text.len() {
		out.push(Piece::Text(&text[plain_start..]));
	}
	out
}

/// The same text with references resolved, for screen readers and reply previews.
pub fn readable(state: &State, message: &Message, clock: &Clock) -> String {
	pieces(state, message, clock)
		.into_iter()
		.map(|piece| match piece {
			Piece::Text(text) => text.to_owned(),
			Piece::Plain(text) | Piece::Mention(text) => text,
		})
		.collect()
}

/// `<:name:id>` or `<a:name:id>`: the emoji name and the bytes the token spans.
fn custom_emoji_prefix(text: &str) -> Option<(&str, usize)> {
	let body = text
		.strip_prefix("<a:")
		.or_else(|| text.strip_prefix("<:"))?;
	let end = body.find('>')?;
	let (name, id) = body[..end].split_once(':')?;
	let valid_name = !name.is_empty()
		&& name.len() <= 32
		&& name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
	let valid_id = !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit());
	(valid_name && valid_id).then(|| (name, text.len() - body.len() + end + 1))
}

/// Up to two initials: the first letter of the first two words, or of the name.
pub fn initials(name: &str) -> String {
	let words: Vec<&str> = name
		.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
		.filter(|word| word.chars().next().is_some_and(char::is_alphanumeric))
		.collect();
	let picked: String = match words.as_slice() {
		[] => name
			.chars()
			.filter(|c| !c.is_whitespace())
			.take(1)
			.collect(),
		[one] => one.chars().take(1).collect(),
		[first, second, ..] => first
			.chars()
			.take(1)
			.chain(second.chars().take(1))
			.collect(),
	};
	picked.to_uppercase()
}

/// Servers in the user's sidebar order: folders first-to-last, then any server the
/// folder settings do not mention, in READY order.
pub fn guilds(state: &State, current: Option<Id>) -> Vec<GuildRow> {
	let mut ordered: Vec<Id> = Vec::with_capacity(state.guilds.len());
	if let Some(settings) = &state.guild_folders {
		for folder in &settings.folders {
			for id in &folder.guild_ids {
				if state.guilds.iter().any(|g| g.id == *id) && !ordered.contains(id) {
					ordered.push(*id);
				}
			}
		}
	}
	for guild in &state.guilds {
		if !ordered.contains(&guild.id) {
			ordered.push(guild.id);
		}
	}
	ordered
		.into_iter()
		.filter_map(|id| state.guilds.iter().find(|g| g.id == id))
		.map(|guild| GuildRow {
			id: guild.id,
			name: guild.name.clone(),
			initials: initials(&guild.name),
			color: avatar_color(guild.id),
			selected: current == Some(guild.id),
			unread: state
				.channels
				.iter()
				.filter(|c| c.guild == Some(guild.id) && state.can_view(c.id))
				.any(|c| channel_unread(state, c)),
		})
		.collect()
}

fn channel_unread(state: &State, channel: &Channel) -> bool {
	state.selected != Some(channel.id) && state.channel_unread(channel).unwrap_or(false)
}

/// A server's channels as Discord's sidebar lists them: uncategorized channels first, then
/// each category with its text-like channels before its voice channels, by position.
pub fn guild_channels(state: &State, guild: Id, collapsed: &BTreeSet<Id>) -> Vec<ChannelRow> {
	let visible = |c: &&Channel| {
		c.guild == Some(guild)
			&& !matches!(c.kind, 10..=12)
			&& channel_kind(c.kind).is_some()
			&& (c.kind == 4 || state.can_view(c.id))
	};
	let order = |c: &Channel| (matches!(c.kind, 2 | 13), c.position, c.id.0);
	let mut loose: Vec<&Channel> = state
		.channels
		.iter()
		.filter(visible)
		.filter(|c| c.kind != 4 && c.parent_id.is_none_or(|p| !is_category(state, p)))
		.collect();
	loose.sort_by_key(|c| order(c));
	let mut categories: Vec<&Channel> = state
		.channels
		.iter()
		.filter(visible)
		.filter(|c| c.kind == 4)
		.collect();
	categories.sort_by_key(|c| (c.position, c.id.0));

	let mut rows: Vec<ChannelRow> = loose.into_iter().map(|c| row(state, c, false)).collect();
	for category in categories {
		let mut children: Vec<&Channel> = state
			.channels
			.iter()
			.filter(visible)
			.filter(|c| c.kind != 4 && c.parent_id == Some(category.id))
			.collect();
		// Discord hides a category whose channels this account cannot see.
		if children.is_empty() {
			continue;
		}
		children.sort_by_key(|c| order(c));
		let folded = collapsed.contains(&category.id);
		rows.push(ChannelRow {
			name: category.name.to_uppercase(),
			collapsed: folded,
			..row(state, category, false)
		});
		for child in children {
			// A folded category still shows the open channel and unread ones, as Discord does.
			let shown = !folded || state.selected == Some(child.id) || channel_unread(state, child);
			if shown {
				rows.push(row(state, child, false));
			}
		}
	}
	rows
}

fn is_category(state: &State, id: Id) -> bool {
	state.channel(id).is_some_and(|c| c.kind == 4)
}

/// Direct messages and groups, most recently active first.
pub fn direct_channels(state: &State) -> Vec<ChannelRow> {
	let mut channels: Vec<&Channel> = state
		.channels
		.iter()
		.filter(|c| c.guild.is_none() && matches!(c.kind, 1 | 3))
		.collect();
	channels.sort_by_key(|c| std::cmp::Reverse(c.last_message.unwrap_or(c.id).0));
	channels.into_iter().map(|c| row(state, c, true)).collect()
}

fn row(state: &State, channel: &Channel, direct: bool) -> ChannelRow {
	let name = if direct {
		state.conversation_name(channel).to_owned()
	} else {
		channel.name.clone()
	};
	let subtitle = if channel.kind == 3 {
		let count = channel.recipients.len() + 1;
		format!("{count} members")
	} else {
		String::new()
	};
	let face = channel
		.recipients
		.first()
		.map_or(channel.id, |user| user.id);
	ChannelRow {
		id: channel.id,
		initials: initials(&name),
		name,
		kind: channel_kind(channel.kind).unwrap_or(kind::TEXT),
		selected: state.selected == Some(channel.id),
		unread: channel_unread(state, channel),
		collapsed: false,
		subtitle,
		color: avatar_color(face),
	}
}

/// Within this gap, consecutive messages by one author share a header, like Discord.
const GROUP_WINDOW_MS: i64 = 7 * 60 * 1000;

fn author_name<'a>(state: &'a State, message: &'a Message) -> &'a str {
	message
		.author_nick
		.as_deref()
		.filter(|nick| !nick.is_empty())
		.unwrap_or_else(|| state.user_display_name(&message.author))
}

/// Text for Discord's non-default message types; `None` keeps the message's own content.
fn system_text(message: &Message) -> Option<&'static str> {
	Some(match message.kind {
		0 | 19 | 20 | 23 => return None,
		1 => "added someone to the group.",
		2 => "removed someone from the group.",
		3 => "started a call.",
		4 => "changed the channel name.",
		5 => "changed the channel icon.",
		6 => "pinned a message to this channel.",
		7 => "joined the server.",
		8..=11 => "boosted the server!",
		18 => "started a thread.",
		_ => return None,
	})
}

fn excerpt(text: &str, limit: usize) -> String {
	let line = text.lines().next().unwrap_or_default();
	if line.chars().count() > limit {
		let cut: String = line.chars().take(limit).collect();
		format!("{cut}…")
	} else {
		line.to_owned()
	}
}

/// The open conversation's messages, oldest first, followed by sends still in flight.
pub fn messages(state: &State, clock: &Clock) -> Vec<MessageRow> {
	let Some(channel) = state.selected else {
		return Vec::new();
	};
	let me = state.user.as_ref().map(|user| user.id);
	let mut rows = Vec::with_capacity(state.timeline.len() + state.pending.len());
	let mut previous: Option<(Id, i64, bool, String)> = None;
	for message in state.timeline.iter().filter(|m| m.channel == channel) {
		let at = model::threads::snowflake_ms(message.id);
		let day = clock.day_label(at);
		let system = system_text(message).is_some();
		let divider = match &previous {
			Some((_, _, _, previous_day)) if *previous_day == day => String::new(),
			None if !state.older_exhausted => String::new(),
			_ => day.clone(),
		};
		let grouped = divider.is_empty()
			&& message.reply_to.is_none()
			&& !system
			&& previous
				.as_ref()
				.is_some_and(|(author, last, was_system, _)| {
					*author == message.author.id && !was_system && at - *last < GROUP_WINDOW_MS
				});
		let reply = message
			.reply_to
			.map(|target| match state.timeline.get(target) {
				Some(original) => format!(
					"{}: {}",
					author_name(state, original),
					excerpt(&readable(state, original, clock), 80)
				),
				None => "Original message was not loaded".to_owned(),
			})
			.unwrap_or_default();
		let author = author_name(state, message).to_owned();
		let attachments: Vec<String> = message
			.attachments
			.iter()
			.map(|a| format!("📎 {}", a.filename))
			.collect();
		let (content, markdown) = match system_text(message) {
			Some(text) => {
				let text = format!("{author} {text}");
				let markdown = to_markdown(&[Piece::Plain(text.clone())]);
				(text, markdown)
			}
			None => {
				let mut pieces = pieces(state, message, clock);
				let mut text = readable(state, message, clock);
				let files = attachments.join("\n");
				if !files.is_empty() {
					if !text.is_empty() {
						text.push('\n');
						pieces.push(Piece::Text("\n"));
					}
					text.push_str(&files);
					pieces.push(Piece::Plain(files.clone()));
				}
				let markdown = to_markdown(&pieces);
				(text, markdown)
			}
		};
		rows.push(MessageRow {
			id: message.id.0.to_string(),
			initials: initials(&author),
			avatar_color: avatar_color(message.author.id),
			author,
			time: clock.header_time(at),
			short_time: clock.clock_time(at),
			content,
			markdown,
			grouped,
			delivery: 0,
			edited: message.edited,
			divider,
			reply,
			system,
			mentions_me: message.mention_everyone
				|| me.is_some_and(|me| message.mentions.iter().any(|user| user.id == me)),
		});
		previous = Some((message.author.id, at, system, day));
	}
	if let Some(user) = &state.user {
		for pending in state
			.pending
			.iter()
			.filter(|p| p.channel == channel && p.delivery != Delivery::Confirmed)
		{
			rows.push(pending_row(state, user, pending, clock, previous.as_ref()));
		}
	}
	rows
}

fn pending_row(
	state: &State,
	user: &User,
	pending: &client_core::Pending,
	clock: &Clock,
	previous: Option<&(Id, i64, bool, String)>,
) -> MessageRow {
	let now = clock.now_ms();
	let author = state.user_display_name(user).to_owned();
	let mut content = pending.content.clone();
	for file in &pending.attachments {
		if !content.is_empty() {
			content.push('\n');
		}
		content.push_str(&format!("📎 {file}"));
	}
	MessageRow {
		id: format!("pending-{}", pending.nonce),
		initials: initials(&author),
		avatar_color: avatar_color(user.id),
		author,
		time: clock.header_time(now),
		short_time: clock.clock_time(now),
		markdown: to_markdown(&[Piece::Plain(content.clone())]),
		content,
		grouped: pending.reply.is_none()
			&& previous.is_some_and(|(author, _, system, _)| *author == user.id && !system),
		delivery: match pending.delivery {
			Delivery::Sending | Delivery::Confirmed => 1,
			Delivery::Rejected => 2,
			Delivery::Ambiguous => 3,
		},
		edited: false,
		divider: String::new(),
		reply: String::new(),
		system: false,
		mentions_me: false,
	}
}

/// "Alex is typing…", "Alex and Sam are typing…", "Several people are typing…".
pub fn typing_text(state: &State, now: std::time::Instant) -> String {
	let me = state.user.as_ref().map(|user| user.id);
	let names: Vec<String> = state
		.typing_users(now)
		.filter(|id| Some(*id) != me)
		.map(|id| {
			state
				.timeline
				.iter()
				.rev()
				.find(|m| m.author.id == id)
				.map_or_else(
					|| "Someone".to_owned(),
					|m| author_name(state, m).to_owned(),
				)
		})
		.collect();
	match names.as_slice() {
		[] => String::new(),
		[one] => format!("{one} is typing…"),
		[one, two] => format!("{one} and {two} are typing…"),
		[one, two, three] => format!("{one}, {two} and {three} are typing…"),
		_ => "Several people are typing…".to_owned(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn initials_take_two_words_or_one_letter() {
		assert_eq!(initials("Synthetic workspace"), "SW");
		assert_eq!(initials("ops-desk"), "OD");
		assert_eq!(initials("rust"), "R");
		assert_eq!(initials("  "), "");
		assert_eq!(initials("éclair club"), "ÉC");
	}

	#[test]
	fn discord_channel_types_map_to_sidebar_kinds() {
		assert_eq!(channel_kind(0), Some(kind::TEXT));
		assert_eq!(channel_kind(2), Some(kind::VOICE));
		assert_eq!(channel_kind(13), Some(kind::VOICE));
		assert_eq!(channel_kind(4), Some(kind::CATEGORY));
		assert_eq!(channel_kind(15), Some(kind::FORUM));
		assert_eq!(channel_kind(11), Some(kind::THREAD));
		assert_eq!(channel_kind(14), None);
	}

	#[test]
	fn demo_server_lists_categories_with_children_and_hides_empty_ones() {
		let state = test_support::demo_state();
		let guild = state.guilds[0].id;
		let rows = guild_channels(&state, guild, &BTreeSet::new());
		assert!(!rows.is_empty());
		// Every category row is followed by at least one channel of that category.
		for (index, row) in rows.iter().enumerate() {
			if row.kind == kind::CATEGORY {
				let next = rows.get(index + 1).expect("category has a child");
				assert_ne!(next.kind, kind::CATEGORY);
				assert_eq!(row.name, row.name.to_uppercase());
			}
		}
		// Within a category, voice channels come after text-like ones.
		let mut seen_voice = false;
		for row in &rows {
			match row.kind {
				kind::CATEGORY => seen_voice = false,
				kind::VOICE => seen_voice = true,
				_ => assert!(!seen_voice, "{} listed after a voice channel", row.name),
			}
		}
	}

	#[test]
	fn folding_a_category_keeps_the_open_channel_visible() {
		let mut state = test_support::demo_state();
		let guild = state.guilds[0].id;
		let open = state.selected.expect("demo opens a channel");
		let parent = state.channel(open).and_then(|c| c.parent_id);
		let Some(parent) = parent.filter(|p| is_category(&state, *p)) else {
			return;
		};
		let folded = BTreeSet::from([parent]);
		let rows = guild_channels(&state, guild, &folded);
		assert!(rows.iter().any(|r| r.id == open));
		state.selected = None;
		let rows = guild_channels(&state, guild, &folded);
		assert!(rows.iter().any(|r| r.id == parent && r.collapsed));
	}

	#[test]
	fn consecutive_messages_from_one_author_share_a_header() {
		let state = test_support::demo_state();
		let clock = Clock::utc();
		let rows = messages(&state, &clock);
		assert!(!rows.is_empty());
		assert!(!rows[0].grouped, "the first row always has a header");
		for pair in rows.windows(2) {
			if pair[1].grouped {
				assert_eq!(pair[0].author, pair[1].author);
				assert!(pair[1].divider.is_empty());
			}
		}
	}

	#[test]
	fn references_read_as_names() {
		let state = test_support::demo_state();
		let clock = Clock::utc();
		let mut message = state.timeline.iter().next().expect("demo history").clone();
		let channel = state
			.channels
			.iter()
			.find(|c| c.guild.is_some() && c.kind == 0)
			.expect("demo text channel");
		message.content = format!(
			"<@{}> see <#{}> <:wave:123> <t:1420070400:f> <not a tag> a<b",
			message.author.id.0, channel.id.0
		);
		let text = readable(&state, &message, &clock);
		let author = author_name(&state, &message).to_owned();
		assert_eq!(
			text,
			format!(
				"@{author} see #{} :wave: 01/01/2015 12:00 AM <not a tag> a<b",
				channel.name
			)
		);
	}

	#[test]
	fn typing_names_are_joined_like_discord() {
		let state = test_support::demo_state();
		assert_eq!(typing_text(&state, std::time::Instant::now()), "");
	}
}
