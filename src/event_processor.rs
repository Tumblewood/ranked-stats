use crate::analysis_types::{RelevantEvent, MatchResult, StatConfig};
use crate::events_reader::{Event, EventsReader, Team};
use crate::log_reader::MatchLog;
use num_traits::FromPrimitive;

const MINIMUM_RANKED_MATCH_LENGTH: usize = 180 * 60;

pub fn process_ranked_match<C: StatConfig>(
    match_log: &MatchLog,
) -> Option<(MatchResult<C::Stats>, Vec<String>)> {
    // Filter matches like the original code
    if !match_log.official
        || match_log.players.len() < 8
        || match_log.group != Some("".to_string())
        || match_log.time_limit != 8.0
        || match_log.duration < MINIMUM_RANKED_MATCH_LENGTH
    {
        return None;
    }

    // Collect relevant events from all players
    let mut relevant_events: Vec<RelevantEvent> = Vec::new();
    let mut player_stats: Vec<C::Stats> = Vec::new();
    
    // Initialize player stats
    for _player in match_log.players.iter() {
        player_stats.push(C::Stats::default());
    }
    
    let mut red_team: Vec<usize> = Vec::new();
    let mut blue_team: Vec<usize> = Vec::new();

    // Collect events from each player
    for (player_idx, player) in match_log.players.iter().enumerate() {
        let player_events = EventsReader::new(player.events.clone())
            .player_events(
                Team::from_usize(player.team).expect("Could not parse Team enum."),
                match_log.duration,
            );
        
        // Track team membership
        match Team::from_usize(player.team).expect("Could not parse Team enum.") {
            Team::Red => red_team.push(player_idx),
            Team::Blue => blue_team.push(player_idx),
            _ => {}
        }

        // Convert player events to relevant events if they match our config
        for event in player_events {
            if C::RELEVANT_EVENTS.contains(&event.event_type) {
                relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: event.event_type,
                    player_index: player_idx,
                    team: event.team,
                });
            }
        }
    }

    // Sort all events by time (your unified timeline approach)
    relevant_events.sort_unstable_by_key(|x| x.time);

    // Process events in chronological order
    let mut cap_diff: isize = 0;
    let mut garbage_time_cap_diff: isize = 0;
    let mut red_fc: Option<usize> = None;
    let mut blue_fc: Option<usize> = None;
    let mut red_grab_time: Option<usize> = None;
    let mut blue_grab_time: Option<usize> = None;
    
    for event in relevant_events.iter() {
        // Handle team changes from Join/Quit events first
        match event.event_type {
            Event::Join => {
                if !red_team.contains(&event.player_index) 
                    && !blue_team.contains(&event.player_index) 
                {
                    match event.team {
                        Team::Red => red_team.push(event.player_index),
                        Team::Blue => blue_team.push(event.player_index),
                        _ => {}
                    }
                }
            }
            Event::Quit => {
                // Handle early quits - remove from team if they quit too early
                if event.time < match_log.duration - MINIMUM_RANKED_MATCH_LENGTH {
                    match event.team {
                        Team::Red => red_team.retain(|&x| x != event.player_index),
                        Team::Blue => blue_team.retain(|&x| x != event.player_index),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        
        // Process the event using the config
        C::process_event(
            event,
            &mut cap_diff,
            &mut garbage_time_cap_diff,
            match_log.duration,
            &mut red_fc,
            &mut blue_fc,
            &mut red_grab_time,
            &mut blue_grab_time,
            &mut player_stats,
        );
    }

    // Only return results for valid 4v4 matches
    if red_team.len() == 4 && blue_team.len() == 4 {
        // Extract player names
        let player_names: Vec<String> = match_log.players.iter()
            .map(|p| p.name.clone())
            .collect();
            
        let result = MatchResult {
            timestamp: match_log.date,
            map_id: match_log.map_id,
            duration: match_log.duration,
            cap_diff,
            garbage_time_cap_diff,
            red_team,
            blue_team,
            player_stats,
        };
        
        Some((result, player_names))
    } else {
        None
    }
}