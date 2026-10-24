//! Local wall-clock time for calendars, crews and outsourcing.
//!
//! The engine schedules on absolute instants (epoch milliseconds, UTC), but a
//! factory's shifts, breaks, holidays and working days are wall-clock facts of
//! a time zone: "08:00 to 17:00 in Asia/Seoul". [`UtcOffsets`] carries the
//! zone as a schedule of offsets from UTC — one base offset plus the instants
//! at which it changes (daylight saving). The engine needs nothing else from
//! the zone database: callers that know the zone (the .NET SDK resolves an IANA
//! name through `TimeZoneInfo`) expand it over the planning horizon.
//!
//! An empty schedule is UTC, which is what every model read before this type
//! existed — so a request that names no zone is scheduled exactly as before.

use serde::{Deserialize, Serialize};

/// Milliseconds in a calendar day.
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// One change of offset: from `at_ms` (UTC) on, local time is UTC + `offset_minutes`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OffsetChange {
    pub at_ms: i64,
    pub offset_minutes: i32,
}

/// A time zone as a schedule of offsets from UTC.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UtcOffsets {
    /// Offset before the first change (minutes east of UTC — Asia/Seoul is 540).
    #[serde(default)]
    pub base_minutes: i32,
    /// Changes in ascending `at_ms`.
    #[serde(default)]
    pub changes: Vec<OffsetChange>,
}

impl UtcOffsets {
    /// UTC — no offset, no changes.
    pub fn utc() -> Self {
        Self::default()
    }

    /// A zone that never changes its offset (Asia/Seoul: `fixed(540)`).
    pub fn fixed(offset_minutes: i32) -> Self {
        Self {
            base_minutes: offset_minutes,
            changes: Vec::new(),
        }
    }

    /// Builder: from `at_ms` (UTC) on, the offset is `offset_minutes`.
    pub fn with_change(mut self, at_ms: i64, offset_minutes: i32) -> Self {
        self.changes.push(OffsetChange {
            at_ms,
            offset_minutes,
        });
        self.changes.sort_by_key(|c| c.at_ms);
        self
    }

    pub fn is_utc(&self) -> bool {
        self.base_minutes == 0 && self.changes.iter().all(|c| c.offset_minutes == 0)
    }

    /// The offset in force at an absolute instant, in milliseconds.
    pub fn offset_ms_at(&self, utc_ms: i64) -> i64 {
        let minutes = self
            .changes
            .iter()
            .take_while(|c| c.at_ms <= utc_ms)
            .last()
            .map_or(self.base_minutes, |c| c.offset_minutes);
        minutes as i64 * 60_000
    }

    /// Absolute instant → local wall-clock milliseconds (a "naive" local time
    /// counted from 1970-01-01 00:00 local).
    pub fn to_local(&self, utc_ms: i64) -> i64 {
        utc_ms + self.offset_ms_at(utc_ms)
    }

    /// Local wall-clock milliseconds → absolute instant. A wall-clock time
    /// skipped by a forward change maps past the gap; one repeated by a
    /// backward change maps to its first occurrence.
    pub fn to_utc(&self, local_ms: i64) -> i64 {
        let mut best: Option<i64> = None;
        let mut candidates = vec![self.base_minutes];
        candidates.extend(self.changes.iter().map(|c| c.offset_minutes));
        for minutes in candidates {
            let utc = local_ms - minutes as i64 * 60_000;
            if self.to_local(utc) == local_ms {
                best = Some(best.map_or(utc, |b: i64| b.min(utc)));
            }
        }
        // In a forward gap no offset round-trips: the wall-clock time does not
        // exist, and the instant it names is the change that skipped it.
        best.unwrap_or_else(|| {
            self.changes
                .iter()
                .map(|c| c.at_ms)
                .find(|&at| self.to_local(at - 1) < local_ms && self.to_local(at) > local_ms)
                .unwrap_or(local_ms - self.base_minutes as i64 * 60_000)
        })
    }
}

/// Days since 1970-01-01 of a local wall-clock time (floor, also before 1970).
pub fn local_day(local_ms: i64) -> i64 {
    local_ms.div_euclid(DAY_MS)
}

/// Local midnight of the day holding `local_ms`.
pub fn local_day_start(local_ms: i64) -> i64 {
    local_day(local_ms) * DAY_MS
}

/// Minute of the local day, 0..1440.
pub fn minute_of_day(local_ms: i64) -> i32 {
    (local_ms.rem_euclid(DAY_MS) / 60_000) as i32
}

/// Day of week of a local day number, Monday = 0 … Sunday = 6 (1970-01-01 was a Thursday).
pub fn weekday_monday0(day: i64) -> u8 {
    (day + 3).rem_euclid(7) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const KST: i32 = 9 * 60;
    // 2027-02-08 00:00 UTC (a Monday)
    const MON_UTC: i64 = 1_802_044_800_000;

    #[test]
    fn utc_is_the_identity() {
        let z = UtcOffsets::utc();
        assert!(z.is_utc());
        assert_eq!(z.to_local(MON_UTC), MON_UTC);
        assert_eq!(z.to_utc(MON_UTC), MON_UTC);
    }

    #[test]
    fn a_fixed_zone_moves_the_day_boundary() {
        let z = UtcOffsets::fixed(KST);
        // 2027-02-07 15:00 UTC is Monday 00:00 in Seoul.
        let seoul_midnight = MON_UTC - 9 * 3_600_000;
        assert_eq!(z.to_local(seoul_midnight), MON_UTC);
        assert_eq!(weekday_monday0(local_day(z.to_local(seoul_midnight))), 0);
        // UTC alone would read that instant as Sunday 15:00.
        assert_eq!(weekday_monday0(local_day(seoul_midnight)), 6);
        assert_eq!(minute_of_day(seoul_midnight), 15 * 60);
        assert_eq!(z.to_utc(MON_UTC), seoul_midnight);
    }

    #[test]
    fn days_and_minutes_are_floored_before_1970() {
        assert_eq!(local_day(-1), -1);
        assert_eq!(minute_of_day(-60_000), 1439);
        assert_eq!(weekday_monday0(-1), 2); // 1969-12-31 was a Wednesday
    }

    #[test]
    fn daylight_saving_changes_round_trip_and_resolve_gaps_and_folds() {
        // A zone at UTC+1 that moves to UTC+2 at 01:00 UTC and back at 03:00 UTC.
        let fwd = 1_000 * DAY_MS + 3_600_000;
        let back = fwd + 2 * 3_600_000;
        let z = UtcOffsets::fixed(60)
            .with_change(fwd, 120)
            .with_change(back, 60);
        for utc in [
            fwd - 60_000,
            fwd,
            fwd + 60_000,
            back - 60_000,
            back + 60_000,
        ] {
            let local = z.to_local(utc);
            // Every instant's wall-clock time maps back to an instant with the same wall-clock time.
            assert_eq!(z.to_local(z.to_utc(local)), local);
        }
        // 02:30 local does not exist on the forward day (02:00 → 03:00): it names the instant after the gap.
        let gap = 1_000 * DAY_MS + 2 * 3_600_000 + 30 * 60_000;
        assert_eq!(z.to_utc(gap), fwd);
        // 04:30 local happens twice on the backward day: the first occurrence.
        let fold = 1_000 * DAY_MS + 4 * 3_600_000 + 30 * 60_000;
        assert_eq!(z.to_utc(fold), fold - 2 * 3_600_000);
    }

    #[test]
    fn it_deserializes_from_nothing_as_utc() {
        let z: UtcOffsets = serde_json::from_str("{}").unwrap();
        assert!(z.is_utc());
    }
}
