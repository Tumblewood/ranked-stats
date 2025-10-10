#[allow(unused_imports)]
use num_traits::FromPrimitive;
use crate::log_reader::{MatchIterator, MatchLog};
use crate::events_reader::{Event, EventsReader, Team};
use std::fs::File;
use std::io::Write;

const OUTPUT_PATH: &str = "ranked/matchups_with_stats.csv";
const MINIMUM_MATCH_LENGTH: usize = 180 * 60;
const FLACCID_GRAB_LENGTH: usize = 2 * 60;

struct RelevantEvent {
    time: usize,
    event_type: Event,
    player_index: usize,
    team: Team
}

struct PlayerStats {
    name: String,
    caps: usize,
    garbage_time_caps: usize,
    hold_start: Option<usize>,
    hold: usize,
    ndps: usize,
    returns: usize,
    quick_returns: usize,
    nrts: usize,
    pups: usize
}

pub fn get_ranked_matchups(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH)
        .unwrap_or(File::open(OUTPUT_PATH).expect("Could not open output file."));

    for (match_id, match_log) in match_iterator {
        if match_log.official &&
            match_log.players.len() >= 8 &&
            match_log.group == Some("".to_string()) &&
            match_log.time_limit == 8.0 &&
            match_log.duration >= MINIMUM_MATCH_LENGTH {
            let mut player_stats: Vec<PlayerStats> = Vec::new();
            let mut red_team: Vec<usize> = Vec::new();
            let mut blue_team: Vec<usize> = Vec::new();

            for (i, player) in match_log.players.iter().enumerate() {
                player_stats.push(PlayerStats {
                    name: player.name.clone(),
                    caps: 0,
                    garbage_time_caps: 0,
                    hold_start: None,
                    hold: 0,
                    quick_returns: 0,
                    ndps: 0,
                    returns: 0,
                    nrts: 0,
                    pups: 0
                });
                match Team::from_usize(player.team).expect("Could not parse Team enum.") {
                    Team::Red => red_team.push(i),
                    Team::Blue => blue_team.push(i),
                    _ => {}
                }
            }

            let mut cap_diff: isize = 0;
            let mut garbage_time_cap_diff: isize = 0;
            let mut red_hold_start: usize = 0;
            let mut blue_hold_start: usize = 0;

            for event in get_relevant_events(&match_log).iter() {
                match event.event_type {
                    Event::Join => {
                        if !red_team.contains(&event.player_index) &&
                            !blue_team.contains(&event.player_index) {
                            match event.team {
                                Team::Red => red_team.push(event.player_index),
                                Team::Blue => blue_team.push(event.player_index),
                                _ => {}
                            };
                        }
                    },
                    Event::Capture => {
                        player_stats[event.player_index].caps += 1;
                        player_stats[event.player_index].hold_start = None;
                        let is_garbage_time: bool = (event.time > 330 * 60 &&
                                (cap_diff >= 4 || cap_diff <= -4)) ||
                                (event.time > 360 * 60 &&
                                (cap_diff >= 3 || cap_diff <= -3)) ||
                                (event.time > 390 * 60 &&
                                (cap_diff >= 2 || cap_diff <= -2));
                        match event.team {
                            Team::Red => {
                                cap_diff += 1;
                                if is_garbage_time && cap_diff > 0 {
                                    garbage_time_cap_diff += 1;
                                    player_stats[event.player_index].garbage_time_caps += 1;
                                }
                            },
                            Team::Blue => {
                                cap_diff -= 1;
                                if is_garbage_time && cap_diff < 0 {
                                    garbage_time_cap_diff -= 1;
                                    player_stats[event.player_index].garbage_time_caps += 1;
                                }
                            },
                            _ => {}
                        }
                    },
                    Event::Grab => {
                        player_stats[event.player_index].hold_start = Some(event.time);
                        match event.team {
                            Team::Red => red_hold_start = event.time,
                            Team::Blue => blue_hold_start = event.time,
                            _ => {}
                        }
                    },
                    Event::Drop => match player_stats[event.player_index].hold_start {
                        Some(hold_start) => {
                            player_stats[event.player_index].hold += event.time - hold_start;
                            player_stats[event.player_index].hold_start = None;
                        },
                        None => {}  // this shouldn't happen
                    },
                    Event::Return => {
                        player_stats[event.player_index].returns += 1;
                        if (event.time - red_hold_start < FLACCID_GRAB_LENGTH && red_team.contains(&event.player_index)) ||
                            (event.time - blue_hold_start < FLACCID_GRAB_LENGTH && blue_team.contains(&event.player_index)) {
                            player_stats[event.player_index].quick_returns += 1;
                        }
                    },
                    Event::Tag => player_stats[event.player_index].nrts += 1,
                    Event::Pop => player_stats[event.player_index].ndps += 1,
                    Event::Powerup => player_stats[event.player_index].pups += 1,
                    Event::DuplicatePowerup => player_stats[event.player_index].pups += 1,
                    Event::Quit => {
                        match player_stats[event.player_index].hold_start {
                            Some(hold_start) => {
                                player_stats[event.player_index].hold += event.time - hold_start;
                                player_stats[event.player_index].hold_start = None;
                            },
                            None => {
                                player_stats[event.player_index].ndps += 1;  // sort of the same effect as a pop
                            }
                        }

                        // if they quit way before the match ends and don't rejoin, ignore the game
                        if event.time < match_log.duration - MINIMUM_MATCH_LENGTH as usize {
                            match event.team {
                                Team::Red => red_team.retain(|&x| x != event.player_index),
                                Team::Blue => blue_team.retain(|&x| x != event.player_index),
                                _ => {}
                            }
                        }
                    },
                    _ => {}
                }
            }

            write_matchup(
                &mut output_file,
                match_id,
                match_log.date,
                match_log.duration,
                match_log.teams[0].score as isize - match_log.teams[1].score as isize,
                garbage_time_cap_diff,
                &red_team,
                &blue_team,
                &player_stats,
            );
        }
    }
}

fn get_relevant_events(match_log: &MatchLog) -> Vec<RelevantEvent> {
    let mut relevant_events: Vec<RelevantEvent> = Vec::new();
    for (i, player) in match_log.players.iter().enumerate() {
        let player_events = EventsReader::new(player.events.clone())
            .player_events(Team::from_usize(player.team).expect("Could not parse Team enum."), match_log.duration);

        for event in player_events {
            match event.event_type {
                Event::Capture => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Capture,
                    player_index: i,
                    team: event.team
                }),
                Event::Grab => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Grab,
                    player_index: i,
                    team: event.team
                }),
                Event::Drop => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Drop,
                    player_index: i,
                    team: event.team
                }),
                Event::Tag => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Tag,
                    player_index: i,
                    team: event.team
                }),
                Event::Return => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Return,
                    player_index: i,
                    team: event.team
                }),
                Event::StartPrevent => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::StartPrevent,
                    player_index: i,
                    team: event.team
                }),
                Event::StopPrevent => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::StopPrevent,
                    player_index: i,
                    team: event.team
                }),
                Event::Pop => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Pop,
                    player_index: i,
                    team: event.team
                }),
                Event::Powerup => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Powerup,
                    player_index: i,
                    team: event.team
                }),
                Event::DuplicatePowerup => relevant_events.push(RelevantEvent {
                    time: event.time,
                    event_type: Event::Powerup,
                    player_index: i,
                    team: event.team
                }),
                _ => {}
            }
        }
    }

    relevant_events.sort_unstable_by_key(|x| x.time);
    return relevant_events;
}

// Write matchup data, including player stats, to the output file.
// date, map_id, duration, cap_diff, then all player names, then all their stats
fn write_matchup(
    output_file: &mut File,
    match_id: String,
    date: usize,
    duration: usize,
    cap_diff: isize,
    garbage_time_cap_diff: isize,
    red_team: &Vec<usize>,
    blue_team: &Vec<usize>,
    player_stats: &Vec<PlayerStats>,
) {
    if red_team.len() != 4 || blue_team.len() != 4 {
        return;
    }

    let mut cells: Vec<String> = vec![
        match_id,
        date.to_string(),
        duration.to_string(),
        cap_diff.to_string(),
        garbage_time_cap_diff.to_string()
    ];
    let current_players: Vec<usize> = vec![red_team.clone(), blue_team.clone()].concat();

    // add player names
    current_players.iter().for_each(|player| {
        cells.push(format!(
            "\"{}\"",
            player_stats[*player].name.escape_default().to_string()
        ));
    });
    // add player stats
    current_players.iter().for_each(|player| {
        cells.push(player_stats[*player].caps.to_string());
        cells.push(player_stats[*player].garbage_time_caps.to_string());
        cells.push(player_stats[*player].hold.to_string());
        cells.push(player_stats[*player].ndps.to_string());
        cells.push(player_stats[*player].returns.to_string());
        cells.push(player_stats[*player].quick_returns.to_string());
        cells.push(player_stats[*player].nrts.to_string());
        cells.push(player_stats[*player].pups.to_string());
    });
    output_file.write_all(
        format!(
            "\n{}",
            cells.join(",")
        ).as_ref()
    ).expect("Could not print matchup to file.");
}
