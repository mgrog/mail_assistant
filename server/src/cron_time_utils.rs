use anyhow::{anyhow, Context};
use chrono::FixedOffset;

pub fn parse_offset_str(offset: &str) -> anyhow::Result<FixedOffset> {
    let (sign, time) = offset.split_at(1);
    let (hours, minutes) = time
        .split_once(':')
        .ok_or_else(|| anyhow!("Invalid offset"))?;
    let seconds_offset = {
        let hours_sec = hours.parse::<i32>().context("Invalid hours")? * 3600;
        let minutes_sec = minutes.parse::<i32>().context("Invalid minutes")? * 60;
        hours_sec + minutes_sec
    };

    if sign == "+" {
        let offset =
            chrono::FixedOffset::west_opt(seconds_offset).context("Could not create offset")?;
        Ok(offset)
    } else {
        let offset =
            chrono::FixedOffset::east_opt(seconds_offset).context("Could not create offset")?;
        Ok(offset)
    }
}
