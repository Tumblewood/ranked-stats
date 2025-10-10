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
    pub keypops: usize,
    pub handoffs: usize,
    pub goodprevent: usize,
    pub resets: usize,
    pub badflaccids: usize,
    pub sparkedouts: usize,
    // Tracking fields for complex stats
    pub prevent_start: Option<usize>,
    pub prevent: usize,
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
        Event::StartPrevent,
        Event::StopPrevent,
        Event::Quit,
    ];
    
    const STAT_FIELDS: &'static [&'static str] = &[
        "caps", "garbage_time_caps", "hold", "ndps", "returns", "quick_returns", "nrts", "pups", 
        "keypops", "handoffs", "goodprevent", "resets", "badflaccids", "sparkedouts"
    ];
    
    fn process_event(
        event: &RelevantEvent,
        cap_diff: &mut isize,
        garbage_time_cap_diff: &mut isize,
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
                        stats.hold += event.time - hold_start;
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
            Event::StartPrevent => {
                stats.prevent_start = Some(event.time);
            }
            Event::StopPrevent => {
                match stats.prevent_start {
                    Some(prevent_start) => {
                        stats.prevent += event.time - prevent_start;
                        stats.prevent_start = None;
                    }
                    None => {} // this shouldn't happen
                }
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
    
    fn post_process_stats(
        all_events: &[RelevantEvent],
        all_player_stats: &mut [Self::Stats],
        red_team: &[usize],
        blue_team: &[usize],
    ) {
        // Process keypops (pops within 2 seconds before an opponent caps)
        for i in 0..all_events.len() {
            if let Event::Capture = all_events[i].event_type {
                let cap_time = all_events[i].time;
                let cap_team = all_events[i].team;
                
                // Look back 2 seconds for pops by opposing team
                for j in (0..i).rev() {
                    if all_events[j].time < cap_time.saturating_sub(2 * 60) { // 2 seconds = 120 ticks
                        break;
                    }
                    if let Event::Pop = all_events[j].event_type {
                        // Check if pop was by opposing team
                        let pop_team = all_events[j].team;
                        if (cap_team == Team::Red && pop_team == Team::Blue) ||
                           (cap_team == Team::Blue && pop_team == Team::Red) {
                            all_player_stats[all_events[j].player_index].keypops += 1;
                        }
                    }
                }
            }
        }
        
        // Process handoffs (drops where teammate grabs within 1 second and caps or holds 5+ seconds)
        for i in 0..all_events.len() {
            if let Event::Drop = all_events[i].event_type {
                let drop_time = all_events[i].time;
                let drop_team = all_events[i].team;
                let drop_player = all_events[i].player_index;
                
                // Look ahead 1 second for grabs by teammates
                for j in (i + 1)..all_events.len() {
                    if all_events[j].time > drop_time + 60 { // 1 second = 60 ticks
                        break;
                    }
                    if let Event::Grab = all_events[j].event_type {
                        let grab_team = all_events[j].team;
                        let grab_player = all_events[j].player_index;
                        
                        // Check if grab was by teammate (same team, different player)
                        if grab_team == drop_team && grab_player != drop_player {
                            // Check if this grab leads to cap or 5+ second hold
                            let mut found_handoff = false;
                            
                            // Look for cap by this player
                            for k in (j + 1)..all_events.len() {
                                if let Event::Capture = all_events[k].event_type {
                                    if all_events[k].player_index == grab_player {
                                        found_handoff = true;
                                        break;
                                    }
                                }
                                // If someone else grabs or caps, this hold ended
                                if matches!(all_events[k].event_type, Event::Grab | Event::Capture) {
                                    break;
                                }
                                // Check for 5+ second hold
                                if all_events[k].time >= all_events[j].time + 5 * 60 { // 5 seconds
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
                            }
                            break; // Only count first teammate grab
                        }
                    }
                }
            }
        }
        
        // Process goodprevent (prevent while no teammate has flag)
        Self::process_goodprevent(all_events, all_player_stats, red_team, blue_team);
        
        // Process resets (returns where no opponent grabs in next 5 seconds and holds 5+ seconds)
        Self::process_resets(all_events, all_player_stats, red_team, blue_team);
        
        // Process badflaccids (drops after <2 seconds where opponent caps in next 10 seconds)
        Self::process_badflaccids(all_events, all_player_stats);
        
        // Process sparkedouts (grabs leading to 5+ seconds hold with no teammate holding in last 3 seconds)
        Self::process_sparkedouts(all_events, all_player_stats, red_team, blue_team);
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
            stats.keypops.to_string(),
            stats.handoffs.to_string(),
            stats.goodprevent.to_string(),
            stats.resets.to_string(),
            stats.badflaccids.to_string(),
            stats.sparkedouts.to_string(),
        ]
    }
}

impl RankedStatConfig {
    fn process_goodprevent(
        all_events: &[RelevantEvent],
        all_player_stats: &mut [RankedPlayerStats],
        red_team: &[usize],
        blue_team: &[usize],
    ) {
        // Track prevent periods and check if team has flag during prevent
        for i in 0..all_events.len() {
            if let Event::StartPrevent = all_events[i].event_type {
                let prevent_start = all_events[i].time;
                let prevent_player = all_events[i].player_index;
                let prevent_team = all_events[i].team;
                
                // Find the corresponding StopPrevent
                for j in (i + 1)..all_events.len() {
                    if let Event::StopPrevent = all_events[j].event_type {
                        if all_events[j].player_index == prevent_player {
                            let prevent_end = all_events[j].time;
                            
                            // Check if any teammate had flag during this prevent period
                            let mut teammate_had_flag = false;
                            let teammate_indices = if prevent_team == Team::Red { red_team } else { blue_team };
                            
                            for k in i..=j {
                                if let Event::Grab = all_events[k].event_type {
                                    if teammate_indices.contains(&all_events[k].player_index) &&
                                       all_events[k].time >= prevent_start &&
                                       all_events[k].time <= prevent_end {
                                        teammate_had_flag = true;
                                        break;
                                    }
                                }
                            }
                            
                            if !teammate_had_flag {
                                all_player_stats[prevent_player].goodprevent += prevent_end - prevent_start;
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
    
    fn process_resets(
        all_events: &[RelevantEvent],
        all_player_stats: &mut [RankedPlayerStats],
        red_team: &[usize],
        blue_team: &[usize],
    ) {
        // Find returns and check if opponents grab and hold for 5+ seconds in next 5 seconds
        for i in 0..all_events.len() {
            if let Event::Return = all_events[i].event_type {
                let return_time = all_events[i].time;
                let return_player = all_events[i].player_index;
                let return_team = all_events[i].team;
                let opponent_indices = if return_team == Team::Red { blue_team } else { red_team };
                
                // Look ahead 5 seconds for opponent grabs
                let mut found_opponent_grab = false;
                for j in (i + 1)..all_events.len() {
                    if all_events[j].time > return_time + 5 * 60 { // 5 seconds
                        break;
                    }
                    
                    if let Event::Grab = all_events[j].event_type {
                        if opponent_indices.contains(&all_events[j].player_index) {
                            // Check if this grab leads to 5+ second hold
                            let grab_time = all_events[j].time;
                            let grab_player = all_events[j].player_index;
                            
                            for k in (j + 1)..all_events.len() {
                                if all_events[k].time >= grab_time + 5 * 60 { // 5 seconds
                                    // Found 5+ second hold
                                    found_opponent_grab = true;
                                    break;
                                }
                                // If flag changes hands, hold ended
                                if matches!(all_events[k].event_type, Event::Grab | Event::Capture | Event::Drop) &&
                                   all_events[k].player_index != grab_player {
                                    break;
                                }
                            }
                            if found_opponent_grab {
                                break;
                            }
                        }
                    }
                }
                
                if !found_opponent_grab {
                    all_player_stats[return_player].resets += 1;
                }
            }
        }
    }
    
    fn process_badflaccids(
        all_events: &[RelevantEvent],
        all_player_stats: &mut [RankedPlayerStats],
    ) {
        // Find drops after <2 seconds of hold where opponent caps in next 10 seconds
        for i in 0..all_events.len() {
            if let Event::Drop = all_events[i].event_type {
                let drop_time = all_events[i].time;
                let drop_player = all_events[i].player_index;
                let drop_team = all_events[i].team;
                
                // Find the corresponding grab to calculate hold time
                let mut hold_time = 0;
                for j in (0..i).rev() {
                    if let Event::Grab = all_events[j].event_type {
                        if all_events[j].player_index == drop_player {
                            hold_time = drop_time - all_events[j].time;
                            break;
                        }
                    }
                }
                
                // Check if hold was <2 seconds
                if hold_time < 2 * 60 { // 2 seconds
                    // Look ahead 10 seconds for opponent caps
                    for j in (i + 1)..all_events.len() {
                        if all_events[j].time > drop_time + 10 * 60 { // 10 seconds
                            break;
                        }
                        
                        if let Event::Capture = all_events[j].event_type {
                            let cap_team = all_events[j].team;
                            // Check if cap was by opposing team
                            if (drop_team == Team::Red && cap_team == Team::Blue) ||
                               (drop_team == Team::Blue && cap_team == Team::Red) {
                                all_player_stats[drop_player].badflaccids += 1;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn process_sparkedouts(
        all_events: &[RelevantEvent],
        all_player_stats: &mut [RankedPlayerStats],
        red_team: &[usize],
        blue_team: &[usize],
    ) {
        // Find grabs leading to 5+ seconds hold with no teammate holding in last 3 seconds
        for i in 0..all_events.len() {
            if let Event::Grab = all_events[i].event_type {
                let grab_time = all_events[i].time;
                let grab_player = all_events[i].player_index;
                let grab_team = all_events[i].team;
                let teammate_indices = if grab_team == Team::Red { red_team } else { blue_team };
                
                // Check if no teammate was holding in last 3 seconds
                let mut teammate_was_holding = false;
                for j in (0..i).rev() {
                    if all_events[j].time < grab_time.saturating_sub(3 * 60) { // 3 seconds
                        break;
                    }
                    
                    if let Event::Grab = all_events[j].event_type {
                        if teammate_indices.contains(&all_events[j].player_index) &&
                           all_events[j].player_index != grab_player {
                            // Check if this teammate was still holding at grab_time
                            let teammate_grab_time = all_events[j].time;
                            let teammate_player = all_events[j].player_index;
                            let mut still_holding = true;
                            
                            for k in (j + 1)..i {
                                if matches!(all_events[k].event_type, Event::Drop | Event::Capture) &&
                                   all_events[k].player_index == teammate_player {
                                    still_holding = false;
                                    break;
                                }
                            }
                            
                            if still_holding && teammate_grab_time <= grab_time {
                                teammate_was_holding = true;
                                break;
                            }
                        }
                    }
                }
                
                if !teammate_was_holding {
                    // Check if this grab leads to 5+ second hold
                    for j in (i + 1)..all_events.len() {
                        if all_events[j].time >= grab_time + 5 * 60 { // 5 seconds
                            all_player_stats[grab_player].sparkedouts += 1;
                            break;
                        }
                        // If flag changes hands, hold ended
                        if matches!(all_events[j].event_type, Event::Grab | Event::Capture | Event::Drop) &&
                           all_events[j].player_index == grab_player {
                            break;
                        }
                    }
                }
            }
        }
    }
}