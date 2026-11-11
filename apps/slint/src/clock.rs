//! Local-time labels for message headers and day dividers.
//!
//! The UTC offset is read once at startup, before the async runtime starts threads, because
//! reading it later is refused on some platforms while other threads run.

use time::{OffsetDateTime, UtcOffset};

pub struct Clock {
	offset: UtcOffset,
}

const MONTHS: [&str; 12] = [
	"January",
	"February",
	"March",
	"April",
	"May",
	"June",
	"July",
	"August",
	"September",
	"October",
	"November",
	"December",
];

impl Clock {
	pub fn local() -> Self {
		Self {
			offset: UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC),
		}
	}

	#[cfg(test)]
	pub fn utc() -> Self {
		Self {
			offset: UtcOffset::UTC,
		}
	}

	pub fn now_ms(&self) -> i64 {
		(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
	}

	fn at(&self, ms: i64) -> OffsetDateTime {
		OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
			.unwrap_or(OffsetDateTime::UNIX_EPOCH)
			.to_offset(self.offset)
	}

	/// "3:05 PM".
	pub fn clock_time(&self, ms: i64) -> String {
		let at = self.at(ms);
		let hour = at.hour();
		let twelve = if hour.is_multiple_of(12) {
			12
		} else {
			hour % 12
		};
		let half = if hour < 12 { "AM" } else { "PM" };
		format!("{twelve}:{:02} {half}", at.minute())
	}

	/// "Today at 3:05 PM", "Yesterday at 3:05 PM", else "11/02/2026 3:05 PM".
	pub fn header_time(&self, ms: i64) -> String {
		let at = self.at(ms);
		let today = self.at(self.now_ms()).date();
		let clock = self.clock_time(ms);
		if at.date() == today {
			format!("Today at {clock}")
		} else if today.previous_day() == Some(at.date()) {
			format!("Yesterday at {clock}")
		} else {
			format!(
				"{:02}/{:02}/{} {clock}",
				u8::from(at.month()),
				at.day(),
				at.year()
			)
		}
	}

	/// "November 11, 2026", the divider between days.
	pub fn day_label(&self, ms: i64) -> String {
		let at = self.at(ms);
		format!(
			"{} {}, {}",
			MONTHS[usize::from(u8::from(at.month())) - 1],
			at.day(),
			at.year()
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn labels_use_twelve_hour_time_and_calendar_days() {
		let clock = Clock::utc();
		// 2026-11-11T00:05:00Z and 2026-11-11T13:45:00Z
		let midnight = 1_794_355_500_000;
		let afternoon = midnight + (13 * 60 + 40) * 60_000;
		assert_eq!(clock.clock_time(midnight), "12:05 AM");
		assert_eq!(clock.clock_time(afternoon), "1:45 PM");
		assert_eq!(clock.day_label(afternoon), "November 11, 2026");
		let old = 1_420_070_400_000; // 2015-01-01T00:00:00Z
		assert_eq!(clock.header_time(old), "01/01/2015 12:00 AM");
		let now = clock.now_ms();
		assert!(clock.header_time(now).starts_with("Today at "));
		assert!(
			clock
				.header_time(now - 24 * 60 * 60 * 1000)
				.starts_with("Yesterday at ")
		);
	}
}
