use crate::analysis_types::{RelevantEvent, StatConfig};
use crate::events_reader::{Event, Team};

#[derive(Debug, Clone, Default)]
pub struct RankedPlayerStats {
    pub name: String,
    pub auth: usize,
    pub caps: usize,
    pub garbage_time_caps: usize,
    pub hold_start: Option<usize>,
    pub hold: usize,
    pub ndps: usize,
    pub returns: usize,
    pub quick_returns: usize,
    pub nrts: usize,
    pub pups: usize,
}

pub struct RankedStatConfig;

impl StatConfig for RankedStatConfig {
    type Stats = RankedPlayerStats;
    
    const RELEVANT_EVENTS: &'static [Event] = &[
        Event::Capture,
        Event::Grab,
        Event::Drop,
        Event::Return,
        Event::Tag,
        Event::Pop,
        Event::Powerup,
        Event::DuplicatePowerup,
        Event::Quit,
    ];
    
    const STAT_FIELDS: &'static [&'static str] = &[
        "caps", "garbage_time_caps", "hold", "ndps", "returns", "quick_returns", "nrts", "pups"
    ];
    
    fn process_event(
        stats: &mut Self::Stats,
        event: &RelevantEvent,
        cap_diff: &mut isize,
        garbage_time_cap_diff: &mut isize,
        match_duration: usize,
    ) {
        match event.event_type {
            Event::Capture => {
                // Check if this is garbage time based on the original logic
                let is_garbage_time = (event.time > 330 * 60 && (*cap_diff >= 4 || *cap_diff <= -4))
                    || (event.time > 360 * 60 && (*cap_diff >= 3 || *cap_diff <= -3))
                    || (event.time > 390 * 60 && (*cap_diff >= 2 || *cap_diff <= -2));
                
                match event.team {
                    Team::Red => {
                        *cap_diff += 1;
                        if is_garbage_time && *cap_diff > 0 {
                            *garbage_time_cap_diff += 1;
                            stats.garbage_time_caps += 1;
                        }
                    }
                    Team::Blue => {
                        *cap_diff -= 1;
                        if is_garbage_time && *cap_diff < 0 {
                            *garbage_time_cap_diff -= 1;
                            stats.garbage_time_caps += 1;
                        }
                    }
                    _ => {}
                }
                stats.caps += 1;
                stats.hold_start = None; // Cap ends hold
            }
            Event::Grab => {
                stats.hold_start = Some(event.time);
            }
            Event::Drop => {
                match stats.hold_start {
                    Some(hold_start) => {
                        stats.hold += event.time - hold_start;
                        stats.hold_start = None;
                    }
                    None => {} // this shouldn't happen
                }
            }
            Event::Return => {
                stats.returns += 1;
                // TODO: Implement quick_returns logic (need team hold start times)
            }
            Event::Tag => {
                stats.nrts += 1;
            }
            Event::Pop => {
                stats.ndps += 1;
            }
            Event::Powerup | Event::DuplicatePowerup => {
                stats.pups += 1;
            }
            Event::Quit => {
                // Handle ongoing hold when player quits
                match stats.hold_start {
                    Some(hold_start) => {
                        stats.hold += event.time - hold_start;
                        stats.hold_start = None;
                    }
                    None => {}
                }
                stats.ndps += 1; // sort of the same effect as a pop
            }
            _ => {}
        }
    }
    
    fn to_csv_values(stats: &Self::Stats) -> Vec<String> {
        vec![
            stats.caps.to_string(),
            stats.garbage_time_caps.to_string(),
            stats.hold.to_string(),
            stats.ndps.to_string(),
            stats.returns.to_string(),
            stats.quick_returns.to_string(),
            stats.nrts.to_string(),
            stats.pups.to_string(),
        ]
    }
}