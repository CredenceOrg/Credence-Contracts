use soroban_sdk::Env;

/// Seconds in a standard day.
pub const SECONDS_PER_DAY: u64 = 86_400;

/// A utility struct for timestamp manipulation.
pub struct Timestamp;

impl Timestamp {
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
            while current_dow >= 5 { // Skip Saturday and Sunday
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
        assert_eq!(Timestamp::add_business_days(thursday, 1), 1 * SECONDS_PER_DAY);
        
        // +2 -> Monday (skip Sat, Sun)
        assert_eq!(Timestamp::add_business_days(thursday, 2), 4 * SECONDS_PER_DAY);
        
        // +5 -> Next Thursday
        assert_eq!(Timestamp::add_business_days(thursday, 5), 7 * SECONDS_PER_DAY);

        // Start on Friday (1970-01-02)
        let friday = 1 * SECONDS_PER_DAY;
        // +1 -> Monday
        assert_eq!(Timestamp::add_business_days(friday, 1), 4 * SECONDS_PER_DAY);
        // +5 -> Next Friday
        assert_eq!(Timestamp::add_business_days(friday, 5), 8 * SECONDS_PER_DAY);

        // Start on Saturday (1970-01-03)
        let saturday = 2 * SECONDS_PER_DAY;
        // +1 -> Monday
        assert_eq!(Timestamp::add_business_days(saturday, 1), 4 * SECONDS_PER_DAY);
        
        // Start on Sunday (1970-01-04)
        let sunday = 3 * SECONDS_PER_DAY;
        // +1 -> Monday
        assert_eq!(Timestamp::add_business_days(sunday, 1), 4 * SECONDS_PER_DAY);
        
        // +10 -> 2 weeks later
        assert_eq!(Timestamp::add_business_days(thursday, 10), 14 * SECONDS_PER_DAY);
    }
}
