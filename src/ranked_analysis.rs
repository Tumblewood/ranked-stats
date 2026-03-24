use crate::analysis_types::{RelevantEvent, StatConfig};
use crate::events_reader::{Event, Team};

#[derive(Debug, Clone, Default)]
pub struct RankedPlayerStats {
    pub name: String,
    pub auth: usize,
    pub caps: usize,
    pub hold_start: Option<usize>,
    pub hold: usize,
    pub earlyhold: usize,
    pub latehold: usize,
    pub ndps: usize,
    pub returns: usize,
    pub quick_returns: usize,
    pub nrts: usize,
    pub pups: usize,
    pub handoffs: usize,
    pub cap_handoffs: usize,
    pub handoffs_received: usize,
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
        "caps", "hold", "earlyhold", "latehold", "ndps", "returns", "quick_returns", "nrts", "pups",
        "handoffs", "cap_handoffs", "handoffs_received"
    ];
    
    fn process_event(
        event: &RelevantEvent,
        cap_diff: &mut isize,
        _match_duration: usize,
        red_fc: &mut Option<usize>,
        blue_fc: &mut Option<usize>,
        red_grab_time: &mut Option<usize>,
        blue_grab_time: &mut Option<usize>,
        all_player_stats: &mut [Self::Stats],
    ) {
        let stats = &mut all_player_stats[event.player_index];
        match event.event_type {
            Event::Capture => {
                stats.caps += 1;

                // Calculate earlyhold and latehold when cap ends hold
                match stats.hold_start {
                    Some(hold_start) => {
                        let hold_time = event.time - hold_start;
                        stats.hold += hold_time;

                        // earlyhold: only count first 5 seconds (300 ticks)
                        stats.earlyhold += hold_time.min(300);

                        // latehold: only count time after 10th second (600 ticks)
                        if hold_time > 600 {
                            stats.latehold += hold_time - 600;
                        }

                        stats.hold_start = None;
                    }
                    None => {}
                }

                // Update cap_diff
                match event.team {
                    Team::Red => *cap_diff += 1,
                    Team::Blue => *cap_diff -= 1,
                    _ => {}
                }

                // Clear flag carrier tracking on capture
                match event.team {
                    Team::Red => {
                        *red_fc = None;
                        *red_grab_time = None;
                    }
                    Team::Blue => {
                        *blue_fc = None;
                        *blue_grab_time = None;
                    }
                    _ => {}
                }
            }
            Event::Grab => {
                stats.hold_start = Some(event.time);
                
                // Track flag carrier for hwoh calculation
                match event.team {
                    Team::Red => {
                        *red_fc = Some(event.player_index);
                        *red_grab_time = Some(event.time);
                    }
                    Team::Blue => {
                        *blue_fc = Some(event.player_index);
                        *blue_grab_time = Some(event.time);
                    }
                    _ => {}
                }
            }
            Event::Drop => {
                match stats.hold_start {
                    Some(hold_start) => {
                        let hold_time = event.time - hold_start;
                        stats.hold += hold_time;

                        // earlyhold: only count first 5 seconds (300 ticks)
                        stats.earlyhold += hold_time.min(300);

                        // latehold: only count time after 10th second (600 ticks)
                        if hold_time > 600 {
                            stats.latehold += hold_time - 600;
                        }

                        stats.hold_start = None;
                    }
                    None => {} // this shouldn't happen
                }

                // Clear flag carrier tracking on drop
                match event.team {
                    Team::Red => {
                        *red_fc = None;
                        *red_grab_time = None;
                    }
                    Team::Blue => {
                        *blue_fc = None;
                        *blue_grab_time = None;
                    }
                    _ => {}
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
                        let hold_time = event.time - hold_start;
                        stats.hold += hold_time;

                        // earlyhold: only count first 5 seconds (300 ticks)
                        stats.earlyhold += hold_time.min(300);

                        // latehold: only count time after 10th second (600 ticks)
                        if hold_time > 600 {
                            stats.latehold += hold_time - 600;
                        }

                        stats.hold_start = None;
                    }
                    None => {}
                }
                stats.ndps += 1; // sort of the same effect as a pop
            }
            _ => {}
        }
    }
    
    fn post_process_stats(
        all_events: &[RelevantEvent],
        all_player_stats: &mut [Self::Stats],
        _red_team: &[usize],
        _blue_team: &[usize],
    ) {
        // handoffs (original): drop → teammate grabs within 1s → teammate caps or holds 5+ seconds
        for i in 0..all_events.len() {
            if let Event::Drop = all_events[i].event_type {
                let drop_time = all_events[i].time;
                let drop_team = all_events[i].team;
                let drop_player = all_events[i].player_index;

                for j in (i + 1)..all_events.len() {
                    if all_events[j].time > drop_time + 60 {
                        break;
                    }
                    if let Event::Grab = all_events[j].event_type {
                        let grab_team = all_events[j].team;
                        let grab_player = all_events[j].player_index;

                        if grab_team == drop_team && grab_player != drop_player {
                            let mut found_handoff = false;

                            for k in (j + 1)..all_events.len() {
                                if let Event::Capture = all_events[k].event_type {
                                    if all_events[k].player_index == grab_player {
                                        found_handoff = true;
                                        break;
                                    }
                                }
                                if matches!(all_events[k].event_type, Event::Grab | Event::Capture) {
                                    break;
                                }
                                if all_events[k].time >= all_events[j].time + 300 {
                                    if let Event::Drop = all_events[k].event_type {
                                        if all_events[k].player_index == grab_player {
                                            found_handoff = true;
                                            break;
                                        }
                                    }
                                }
                            }

                            if found_handoff {
                                all_player_stats[drop_player].handoffs += 1;
                                all_player_stats[grab_player].handoffs_received += 1;
                            }
                            break;
                        }
                    }
                }
            }
        }

        // cap_handoffs: drop → cap by teammate within 2 seconds (120 ticks).
        // Single forward pass: track the most recent drop per team.
        let mut red_drop: Option<(usize, usize)> = None; // (player_index, drop_time)
        let mut blue_drop: Option<(usize, usize)> = None;

        for event in all_events {
            match event.event_type {
                Event::Drop => match event.team {
                    Team::Red => red_drop = Some((event.player_index, event.time)),
                    Team::Blue => blue_drop = Some((event.player_index, event.time)),
                    _ => {}
                },
                Event::Capture => {
                    let pending = match event.team {
                        Team::Red => &mut red_drop,
                        Team::Blue => &mut blue_drop,
                        _ => continue,
                    };
                    if let Some((drop_player, drop_time)) = *pending {
                        if drop_player != event.player_index && event.time <= drop_time + 120 {
                            all_player_stats[drop_player].cap_handoffs += 1;
                        }
                    }
                    *pending = None;
                }
                // Flag returned to base — no longer a live drop for the team that had it
                Event::Return => match event.team {
                    Team::Red => blue_drop = None,
                    Team::Blue => red_drop = None,
                    _ => {}
                },
                _ => {}
            }
        }
    }
    
    fn to_csv_values(stats: &Self::Stats) -> Vec<String> {
        vec![
            stats.caps.to_string(),
            stats.hold.to_string(),
            stats.earlyhold.to_string(),
            stats.latehold.to_string(),
            stats.ndps.to_string(),
            stats.returns.to_string(),
            stats.quick_returns.to_string(),
            stats.nrts.to_string(),
            stats.pups.to_string(),
            stats.handoffs.to_string(),
            stats.cap_handoffs.to_string(),
            stats.handoffs_received.to_string(),
        ]
    }
}