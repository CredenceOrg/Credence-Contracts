/// Seconds in a standard day.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// A utility struct for timestamp manipulation.
pub struct Timestamp;

impl Timestamp {
    /// Truncates a timestamp (in seconds) to the start of its UTC day.
    #[inline]
    #[must_use]
    pub fn floor_to_day(t: u64) -> u64 {
        (t / SECONDS_PER_DAY) * SECONDS_PER_DAY
    }

    /// Adds `n` business days to the given timestamp `t` (in seconds since the UNIX epoch).
    ///
    /// # Assumptions
    /// * **Business calendar**: Monday through Friday are considered business days.
    /// * **Weekends**: Saturdays and Sundays are skipped.
    /// * **Holidays**: Public holidays are *not* accounted for.
    /// * **Time of day**: The time of day in the timestamp is preserved exactly.
    /// * **Epoch**: `t` is based on the UNIX epoch (1970-01-01), which was a Thursday.
    ///
    /// If `t` falls on a weekend, adding 1 business day will advance the timestamp
    /// to the upcoming Monday. If `n = 0`, the timestamp is returned unchanged, even
    /// if it falls on a weekend.
    #[inline]
    #[must_use]
    pub fn add_business_days(t: u64, n: u64) -> u64 {
        if n == 0 {
            return t;
        }

        let time_of_day = t % SECONDS_PER_DAY;
        let days_since_epoch = t / SECONDS_PER_DAY;

        // 1970-01-01 was a Thursday.
        // 0 = Thursday, 1 = Friday, 2 = Saturday, 3 = Sunday, 4 = Monday, 5 = Tuesday, 6 = Wednesday
        let day_of_week = days_since_epoch % 7;

        // Map to standard 0-6 where 0 = Monday, ..., 5 = Saturday, 6 = Sunday
        // Thursday (0) -> 3
        let standard_dow = (day_of_week + 3) % 7;

        let weeks = n / 5;
        let extra_days = n % 5;

        let mut actual_days = weeks * 7;
        let mut current_dow = standard_dow;

        for _ in 0..extra_days {
            actual_days += 1;
            current_dow = (current_dow + 1) % 7;
            while current_dow >= 5 {
                // Skip Saturday and Sunday
                actual_days += 1;
                current_dow = (current_dow + 1) % 7;
            }
        }

        let new_days = days_since_epoch + actual_days;
        new_days * SECONDS_PER_DAY + time_of_day
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_business_days() {
        // 1970-01-01 was a Thursday (0 days since epoch)
        let thursday = 0;

        // +0 -> Thursday
        assert_eq!(Timestamp::add_business_days(thursday, 0), thursday);

        // +1 -> Friday
        assert_eq!(Timestamp::add_business_days(thursday, 1), SECONDS_PER_DAY);

        // +2 -> Monday (skip Sat, Sun)
        assert_eq!(
            Timestamp::add_business_days(thursday, 2),
            4 * SECONDS_PER_DAY
        );

        // +5 -> Next Thursday
        assert_eq!(
            Timestamp::add_business_days(thursday, 5),
            7 * SECONDS_PER_DAY
        );

        // Start on Friday (1970-01-02)
        let friday = SECONDS_PER_DAY;
        // +1 -> Monday
        assert_eq!(Timestamp::add_business_days(friday, 1), 4 * SECONDS_PER_DAY);
        // +5 -> Next Friday
        assert_eq!(Timestamp::add_business_days(friday, 5), 8 * SECONDS_PER_DAY);

        // Start on Saturday (1970-01-03)
        let saturday = 2 * SECONDS_PER_DAY;
        // +1 -> Monday
        assert_eq!(
            Timestamp::add_business_days(saturday, 1),
            4 * SECONDS_PER_DAY
        );

        // Start on Sunday (1970-01-04)
        let sunday = 3 * SECONDS_PER_DAY;
        // +1 -> Monday
        assert_eq!(Timestamp::add_business_days(sunday, 1), 4 * SECONDS_PER_DAY);

        // +10 -> 2 weeks later
        assert_eq!(
            Timestamp::add_business_days(thursday, 10),
            14 * SECONDS_PER_DAY
        );
    }

    #[test]
    fn test_add_business_days_all_weekdays_table() {
        // Table-driven coverage over all 7 weekday starts (epoch 1970-01-01 = Thursday)
        // for every n in 1..=14.
        // Each row is (weekday_name, start_day_since_epoch, expected_day_deltas_for_n_1_to_14).
        // Note: when starting on a weekend and n is a multiple of 5 (n = 5, 10),
        // `extra_days == 0` so `add_business_days` advances by `weeks * 7` calendar days.
        let table: [(&str, u64, [u64; 14]); 7] = [
            (
                "Thursday",
                0,
                [1, 4, 5, 6, 7, 8, 11, 12, 13, 14, 15, 18, 19, 20],
            ),
            (
                "Friday",
                1,
                [3, 4, 5, 6, 7, 10, 11, 12, 13, 14, 17, 18, 19, 20],
            ),
            (
                "Saturday",
                2,
                [2, 3, 4, 5, 7, 9, 10, 11, 12, 14, 16, 17, 18, 19],
            ),
            (
                "Sunday",
                3,
                [1, 2, 3, 4, 7, 8, 9, 10, 11, 14, 15, 16, 17, 18],
            ),
            (
                "Monday",
                4,
                [1, 2, 3, 4, 7, 8, 9, 10, 11, 14, 15, 16, 17, 18],
            ),
            (
                "Tuesday",
                5,
                [1, 2, 3, 6, 7, 8, 9, 10, 13, 14, 15, 16, 17, 20],
            ),
            (
                "Wednesday",
                6,
                [1, 2, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 19, 20],
            ),
        ];

        for (name, start_day, expected_deltas) in table {
            let start_ts = start_day * SECONDS_PER_DAY;
            for (idx, &delta_days) in expected_deltas.iter().enumerate() {
                let n = (idx as u64) + 1;
                let expected = (start_day + delta_days) * SECONDS_PER_DAY;
                assert_eq!(
                    Timestamp::add_business_days(start_ts, n),
                    expected,
                    "failed for start={name} (day {start_day}), n={n}"
                );
            }
        }
    }

    #[test]
    fn test_add_business_days_zero_on_weekend_and_weekdays() {
        let time_of_day = 12 * 3_600 + 34 * 60 + 56;
        for start_day in 0..7u64 {
            let ts = start_day * SECONDS_PER_DAY + time_of_day;
            assert_eq!(Timestamp::add_business_days(ts, 0), ts);
        }
    }

    #[test]
    fn test_add_business_days_preserves_time_of_day() {
        let times_of_day = [0, 1, 32_400, 43_200, 61_199, SECONDS_PER_DAY - 1];
        for start_day in 0..7u64 {
            for &tod in &times_of_day {
                for n in [0u64, 1, 2, 5, 7, 14] {
                    let base = start_day * SECONDS_PER_DAY;
                    let expected = Timestamp::add_business_days(base, n) + tod;
                    let actual = Timestamp::add_business_days(base + tod, n);
                    assert_eq!(
                        actual, expected,
                        "time-of-day not preserved for day={start_day}, tod={tod}, n={n}"
                    );
                    assert_eq!(actual % SECONDS_PER_DAY, tod);
                }
            }
        }
    }

    #[test]
    fn test_add_business_days_near_u64_max_no_overflow() {
        // n = 0 at u64::MAX must return u64::MAX without overflowing.
        assert_eq!(Timestamp::add_business_days(u64::MAX, 0), u64::MAX);

        // Near u64::MAX with room for weekend skip (at most 3 calendar days for n = 1).
        let safe_near_max = u64::MAX - 3 * SECONDS_PER_DAY;
        let result = Timestamp::add_business_days(safe_near_max, 1);
        assert!(result > safe_near_max);
        assert_eq!(result % SECONDS_PER_DAY, safe_near_max % SECONDS_PER_DAY);
    }

    #[test]
    #[should_panic]
    fn test_add_business_days_overflow_near_u64_max_panics() {
        let _ = Timestamp::add_business_days(u64::MAX, 1);
    }

    #[test]
    #[should_panic]
    fn test_add_business_days_overflow_large_n_panics() {
        let _ = Timestamp::add_business_days(0, u64::MAX);
    }
}
