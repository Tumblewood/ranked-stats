#[allow(unused_imports)]
use num_traits::FromPrimitive;
use crate::log_reader::MatchIterator;
use crate::events_reader::{Event, EventsReader, Powerup, Team};
use std::fs::File;
use std::io::Write;

const TIME_AFTER_JOIN_TO_IGNORE: isize = 10 * 60;
const MINIMUM_MATCHUP_LENGTH: isize = 31 * 60;
const RESPAWN_DURATION: isize = 3 * 60;
const MINIMUM_RANKED_MATCH_LENGTH: usize = 180 * 60;

const CSV_HEADER_WITHOUT_STATS: &str = "timestamp,map,duration,diff,r1,r2,r3,r4,b1,b2,b3,b4";
const CSV_HEADER_WITH_STATS: &str = "timestamp,map,duration,diff,r1,r2,r3,r4,b1,b2,b3,b4,r1_caps,r1_hold,r1_returns,r1_ndps,r1_pups,r2_caps,r2_hold,r2_returns,r2_ndps,r2_pups,r3_caps,r3_hold,r3_returns,r3_ndps,r3_pups,r4_caps,r4_hold,r4_returns,r4_ndps,r4_pups,b1_caps,b1_hold,b1_returns,b1_ndps,b1_pups,b2_caps,b2_hold,b2_returns,b2_ndps,b2_pups,b3_caps,b3_hold,b3_returns,b3_ndps,b3_pups,b4_caps,b4_hold,b4_returns,b4_ndps,b4_pups";
const CSV_HEADER_PUP_TIMES: &str = "match_id,timestamp,map,player,pup_type,time\n";
const CSV_HEADER_CAP_TIMES: &str = "match_id,timestamp,map,player,time\n";
const CSV_HEADER_RANKED_WITHOUT_STATS: &str = "timestamp,map,duration,red,blue,r1,r2,r3,r4,b1,b2,b3,b4";
const CSV_HEADER_RANKED_WITH_STATS: &str = "timestamp,map,duration,diff,r1,r2,r3,r4,b1,b2,b3,b4,r1_caps,r1_hold,r1_returns,r1_prevent,r1_ndps,r1_pups,r2_caps,r2_hold,r2_returns,r2_prevent,r2_ndps,r2_pups,r3_caps,r3_hold,r3_returns,r3_prevent,r3_ndps,r3_pups,r4_caps,r4_hold,r4_returns,r4_prevent,r4_ndps,r4_pups,b1_caps,b1_hold,b1_returns,b1_prevent,b1_ndps,b1_pups,b2_caps,b2_hold,b2_returns,b2_prevent,b2_ndps,b2_pups,b3_caps,b3_hold,b3_returns,b3_prevent,b3_ndps,b3_pups,b4_caps,b4_hold,b4_returns,b4_prevent,b4_ndps,b4_pups";
const OUTPUT_PATH_WITHOUT_STATS: &str = "ratings/matchups.csv";
const OUTPUT_PATH_WITH_STATS: &str = "ratings/matchups_with_stats.csv";
const OUTPUT_PATH_PUP_TIMES: &str = "analysis/pup_times.csv";
const OUTPUT_PATH_CAP_TIMES: &str = "analysis/cap_times.csv";
const OUTPUT_PATH_RANKED_WITHOUT_STATS: &str = "analysis/matchups.csv";
const OUTPUT_PATH_RANKED_WITH_STATS: &str = "analysis/matchups_with_stats.csv";

struct RelevantEvent {
    time: usize,
    event_type: Event,
    player_index: usize,
    team: Team
}

struct PlayerStats {
    name: String,
    auth: usize,
    caps: usize,
    hold_start: Option<usize>,
    hold: usize,
    returns: usize,
    prevent_start: Option<usize>,
    prevent: usize,
    ndps: usize,
    pups: usize
}

pub fn get_ranked_matchups_no_stats(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH_RANKED_WITHOUT_STATS)
        .unwrap_or(File::open(OUTPUT_PATH_RANKED_WITHOUT_STATS).expect("Could not open output file."));
    output_file.write_all(CSV_HEADER_RANKED_WITHOUT_STATS.as_ref()).expect("Could not write header to file.");

    for (_match_id, match_log) in match_iterator {
        if match_log.official &&
            match_log.players.len() >= 8 &&
            match_log.group == Some("".to_string()) &&
            match_log.time_limit == 8.0 &&
            match_log.duration >= MINIMUM_RANKED_MATCH_LENGTH {
            let mut red_team: Vec<String> = Vec::new();
            let mut blue_team: Vec<String> = Vec::new();

            for player in match_log.players.iter() {
                match Team::from_usize(player.team).unwrap_or(Team::None) {
                    Team::Red => red_team.push(player.name.to_string()),
                    Team::Blue => blue_team.push(player.name.to_string()),
                    _ => {}
                }
            }

            if red_team.len() == 4 && blue_team.len() == 4 {
                let cells: Vec<String> = vec![
                    match_log.date.to_string(),
                    match_log.map_id.to_string(),
                    match_log.duration.to_string(),
                    match_log.teams[0].score.to_string(),
                    match_log.teams[1].score.to_string(),
                ]
                .into_iter()
                .chain(red_team.into_iter())
                .chain(blue_team.into_iter())
                .collect();

                output_file.write_all(
                    format!("\n{}", cells.join(",")).as_bytes()
                ).expect("Could not print matchup to file.");
            }
        }
    }
}

pub fn get_ranked_matchups_with_stats(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH_RANKED_WITH_STATS)
        .unwrap_or(File::open(OUTPUT_PATH_RANKED_WITH_STATS).expect("Could not open output file."));
    output_file.write_all(CSV_HEADER_RANKED_WITH_STATS.as_ref()).expect("Could not write header to file.");

    for (_match_id, match_log) in match_iterator {
        if match_log.official &&
            match_log.players.len() >= 8 &&
            match_log.group == Some("".to_string()) &&
            match_log.time_limit == 8.0 &&
            match_log.duration >= MINIMUM_RANKED_MATCH_LENGTH {
            let mut relevant_events: Vec<RelevantEvent> = Vec::new();
            let mut player_stats: Vec<PlayerStats> = Vec::new();
            for player in match_log.players.iter() {
                player_stats.push(PlayerStats {
                    name: player.name.clone(),
                    auth: player.auth as usize,
                    caps: 0,
                    hold_start: None,
                    hold: 0,
                    returns: 0,
                    prevent_start: None,
                    prevent: 0,
                    ndps: 0,
                    pups: 0
                });
            }
            let mut red_team: Vec<usize> = Vec::new();
            let mut blue_team: Vec<usize> = Vec::new();

            for (i, player) in match_log.players.iter().enumerate() {
                let player_events = EventsReader::new(player.events.clone())
                    .player_events(Team::from_usize(player.team).expect("Could not parse Team enum."), match_log.duration);
                match Team::from_usize(player.team).expect("Could not parse Team enum.") {
                    Team::Red => red_team.push(i),
                    Team::Blue => blue_team.push(i),
                    _ => {}
                }

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

            let mut cap_diff: isize = 0;

            for event in relevant_events.iter() {
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
                        cap_diff += match event.team {
                            Team::Red => 1,
                            Team::Blue => -1,
                            _ => 0
                        };
                        player_stats[event.player_index].caps += 1;
                    },
                    Event::Grab => player_stats[event.player_index].hold_start = Some(event.time),
                    Event::Drop => match player_stats[event.player_index].hold_start {
                        Some(hold_start) => {
                            player_stats[event.player_index].hold += event.time - hold_start;
                            player_stats[event.player_index].hold_start = None;
                        },
                        None => {}  // this shouldn't happen
                    },
                    Event::Return => player_stats[event.player_index].returns += 1,
                    Event::StartPrevent => {
                        player_stats[event.player_index].prevent_start = Some(event.time);
                    },
                    Event::StopPrevent => match player_stats[event.player_index].prevent_start {
                        Some(prevent_start) => {
                            player_stats[event.player_index].prevent += event.time - prevent_start;
                            player_stats[event.player_index].prevent_start = None;
                        },
                        None => {}  // this shouldn't happen
                    },
                    Event::Pop => player_stats[event.player_index].ndps += 1,
                    Event::Powerup => player_stats[event.player_index].pups += 1,
                    Event::DuplicatePowerup => player_stats[event.player_index].pups += 1,
                    Event::Quit => {
                        match player_stats[event.player_index].hold_start {
                            Some(hold_start) => {
                                player_stats[event.player_index].hold += event.time - hold_start;
                                player_stats[event.player_index].hold_start = None;
                            },
                            None => {}
                        }
                        player_stats[event.player_index].ndps += 1;  // sort of the same effect as a pop
                        // if they quit way before the match ends and don't rejoin, ignore the game
                        if event.time < match_log.duration - MINIMUM_RANKED_MATCH_LENGTH as usize {
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

            write_ranked_matchup_with_stats(
                &mut output_file,
                match_log.date,
                match_log.map_id,
                match_log.duration,
                cap_diff,
                &red_team,
                &blue_team,
                &player_stats,
            );
        }
    }
}

pub fn log_cap_times(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH_CAP_TIMES).expect("Could not create output file.");
    output_file.write_all(CSV_HEADER_CAP_TIMES.as_ref()).expect("Could not write header to file.");

    for (match_id, match_log) in match_iterator {
        if match_log.official &&
            match_log.players.len() >= 8 &&
            match_log.group == Some("".to_string()) &&
            match_log.time_limit == 8.0 &&
            match_log.duration >= MINIMUM_RANKED_MATCH_LENGTH {
            for player in match_log.players.iter() {
                let player_events = EventsReader::new(player.events.clone())
                    .player_events(Team::from_usize(player.team).expect("Could not parse Team enum."), match_log.duration);

                for event in player_events {
                    if event.event_type == Event::Capture {
                        output_file.write_all(format!("{},{},{},\"{}\",{}\n",
                            match_id, match_log.date, match_log.map_id, player.name, event.time
                        ).as_bytes()).expect("Could not write to output file.");
                    }
                }
            }
        }
    }
}

pub fn log_pup_times(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH_PUP_TIMES).expect("Could not create output file.");
    output_file.write_all(CSV_HEADER_PUP_TIMES.as_ref()).expect("Could not write header to file.");
    
    for (match_id, match_log) in match_iterator {
        if match_log.official &&
            match_log.players.len() >= 8 &&
            match_log.group == Some("".to_string()) &&
            match_log.time_limit == 8.0 &&
            match_log.duration >= MINIMUM_RANKED_MATCH_LENGTH {
            for player in match_log.players.iter() {
                let player_events = EventsReader::new(player.events.clone())
                    .player_events(Team::from_usize(player.team).expect("Could not parse Team enum."), match_log.duration);
                let mut current_pups: usize = 0;

                for event in player_events {
                    match event.event_type {
                        Event::Powerup => {
                            // Find which powerup the player has that they didn't before by comparing event.powerups to current_pups
                            let new_pup: Powerup = Powerup::from_usize(event.powerups - current_pups).unwrap_or(Powerup::None);
                            current_pups = event.powerups;
                            match new_pup {
                                Powerup::TagPro => {
                                    output_file.write_all(format!("{},{},{},\"{}\",tp,{}\n",
                                        match_id, match_log.date, match_log.map_id, player.name, event.time
                                    ).as_bytes()).expect("Could not write to output file.");
                                },
                                Powerup::JukeJuice => {
                                    output_file.write_all(format!("{},{},{},\"{}\",jj,{}\n",
                                        match_id, match_log.date, match_log.map_id, player.name, event.time
                                    ).as_bytes()).expect("Could not write to output file.");
                                },
                                Powerup::RollingBomb => {
                                    output_file.write_all(format!("{},{},{},\"{}\",rb,{}\n",
                                        match_id, match_log.date, match_log.map_id, player.name, event.time
                                    ).as_bytes()).expect("Could not write to output file.");
                                },
                                _ => continue
                            }
                        },
                        Event::DuplicatePowerup => {
                            current_pups = event.powerups;
                            // If the player only has one powerup, we know that one is the
                            // duplicate they just picked up, so log it. Otherwise, log as "du".
                            let new_pup: Powerup = Powerup::from_usize(event.powerups).unwrap_or(Powerup::TopSpeed);
                            match new_pup {
                                Powerup::TagPro | Powerup::TopSpeed => {
                                    // if we don't know what duplicate it is, just log it as a TP
                                    output_file.write_all(format!("{},{},{},\"{}\",tp,{}\n",
                                        match_id, match_log.date, match_log.map_id, player.name, event.time
                                    ).as_bytes()).expect("Could not write to output file.");
                                },
                                Powerup::JukeJuice => {
                                    output_file.write_all(format!("{},{},{},\"{}\",jj,{}\n",
                                        match_id, match_log.date, match_log.map_id, player.name, event.time
                                    ).as_bytes()).expect("Could not write to output file.");
                                },
                                // If the player shows as having no powerups, it means they
                                // picked up a rolling bomb and it was defused in the same tick
                                Powerup::RollingBomb | Powerup::None => {
                                    output_file.write_all(format!("{},{},{},\"{}\",rb,{}\n",
                                        match_id, match_log.date, match_log.map_id, player.name, event.time
                                    ).as_bytes()).expect("Could not write to output file.");
                                }
                            }
                        },
                        _ => {
                            current_pups = event.powerups;
                        }
                    }
                }
            }
        }
    }
}

pub fn get_matchups_with_stats(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH_WITH_STATS).unwrap_or(
        File::open(OUTPUT_PATH_WITH_STATS).expect("Could not open output file.")
    );
    output_file.write_all(CSV_HEADER_WITH_STATS.as_ref()).expect("Could not write header to file.");
    for (_match_id, match_log) in match_iterator {
        if match_log.official &&
                match_log.players.len() >= 8 &&
                match_log.group != Some("redacted".to_string()) {
            let mut relevant_events: Vec<RelevantEvent> = Vec::new();
            let mut player_stats: Vec<PlayerStats> = Vec::new();
            for player in match_log.players.iter() {
                player_stats.push(PlayerStats {
                    name: player.name.clone(),
                    auth: player.auth as usize,
                    caps: 0,
                    hold_start: None,
                    hold: 0,
                    returns: 0,
                    prevent_start: None,
                    prevent: 0,
                    ndps: 0,
                    pups: 0
                });
            }

            for (i, player) in match_log.players.iter().enumerate() {
                let player_events = EventsReader::new(player.events.clone())
                    .player_events(Team::from_usize(player.team).expect("Could not parse Team enum."), match_log.duration);

                // If the player is on a team at the start of the match, add a join event.
                if Team::from_usize(player.team).expect("Could not parse Team enum.") != Team::None {
                    relevant_events.push(RelevantEvent {
                        time: 0,
                        event_type: Event::Join,
                        player_index: i,
                        team: Team::from_usize(player.team).expect("Could not parse Team enum.")
                    });
                }

                // track relevent events
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
                        Event::Pop => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Pop,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Drop => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Drop,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Return => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Return,
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
                        Event::Join => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Join,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Quit => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Quit,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Switch => {
                            relevant_events.push(RelevantEvent {
                                time: event.time,
                                event_type: Event::Join,
                                player_index: i,
                                team: event.team
                            });
                            relevant_events.push(RelevantEvent {
                                time: event.time,
                                event_type: Event::Quit,
                                player_index: i,
                                team: match event.team {
                                    Team::Red => Team::Blue,
                                    Team::Blue => Team::Red,
                                    _ => Team::None
                                }
                            });
                        },
                        _ => {}
                    };
                }
            }

            relevant_events.sort_unstable_by_key(|x| x.time);

            let mut red_team: Vec<usize> = Vec::new();
            let mut blue_team: Vec<usize> = Vec::new();
            let mut last_join_time: isize = -TIME_AFTER_JOIN_TO_IGNORE - RESPAWN_DURATION;
            let mut cap_diff: isize = 0;

            for event in relevant_events.iter() {
                match event.event_type {
                    Event::Join => {
                        match event.team {
                            Team::Red => red_team.push(event.player_index),
                            Team::Blue => blue_team.push(event.player_index),
                            _ => {}
                        }
                        cap_diff = 0;
                        last_join_time = event.time as isize;
                        player_stats.iter_mut().for_each(|player| {
                            player.caps = 0;
                            player.hold_start = None;
                            player.hold = 0;
                            player.returns = 0;
                            player.ndps = 0;
                            player.pups = 0;
                        });
                    },
                    Event::Quit => {
                        if red_team.len() == 4 &&
                            blue_team.len() == 4 &&
                            event.time as isize > (last_join_time + MINIMUM_MATCHUP_LENGTH) {
                            write_matchup_with_stats(
                                &mut output_file, match_log.date, match_log.map_id, event.time - (last_join_time as usize), cap_diff,
                                &red_team, &blue_team, &player_stats
                            );
                        }
                        match event.team {
                            Team::Red => {red_team.retain(|&x| x != event.player_index);},
                            Team::Blue => {blue_team.retain(|&x| x != event.player_index);},
                            _ => {}
                        };
                    },
                    Event::Switch => {
                        match event.team {
                            Team::Red => {
                                red_team.retain(|&x| x != event.player_index);
                                blue_team.push(event.player_index);
                            },
                            Team::Blue => {
                                blue_team.retain(|&x| x != event.player_index);
                                red_team.push(event.player_index);
                            },
                            _ => {}
                        }
                    }
                    _ => {}
                }

                if event.time as isize > (last_join_time + TIME_AFTER_JOIN_TO_IGNORE) {
                    match event.event_type {
                        Event::Capture => {
                            cap_diff += match event.team {
                                Team::Red => 1,
                                Team::Blue => -1,
                                _ => 0
                            };
                            player_stats[event.player_index].caps += 1;
                        },
                        Event::Grab => player_stats[event.player_index].hold_start = Some(event.time),
                        Event::Drop => match player_stats[event.player_index].hold_start {
                            Some(hold_start) => {
                                player_stats[event.player_index].hold += event.time - hold_start;
                                player_stats[event.player_index].hold_start = None;
                            },
                            None => {
                                player_stats[event.player_index].hold += event.time - (last_join_time + TIME_AFTER_JOIN_TO_IGNORE) as usize;
                            }
                        },
                        Event::Return => player_stats[event.player_index].returns += 1,
                        Event::Pop => player_stats[event.player_index].ndps += 1,
                        Event::Powerup => player_stats[event.player_index].pups += 1,
                        Event::DuplicatePowerup => player_stats[event.player_index].pups += 1,
                        _ => {}
                    }
                }
            }

            if red_team.len() == 4 &&
                blue_team.len() == 4 &&
                match_log.duration as isize > (last_join_time + MINIMUM_MATCHUP_LENGTH) {
                write_matchup_with_stats(
                    &mut output_file, match_log.date, match_log.map_id, match_log.duration - last_join_time as usize + RESPAWN_DURATION as usize, cap_diff,
                    &red_team, &blue_team, &player_stats
                );
            }
        }
    }
}

pub fn get_matchups_without_stats(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH_WITHOUT_STATS).unwrap_or(
        File::open(OUTPUT_PATH_WITHOUT_STATS).expect("Could not open output file.")
    );
    output_file.write_all(CSV_HEADER_WITHOUT_STATS.as_ref()).expect("Could not write header to file.");
    for (_match_id, match_log) in match_iterator {
        // Filter to public games with 8+ players that weren't in a group
        if match_log.official &&
                match_log.players.len() >= 8 &&
                match_log.group == Some("".to_string()) {
            let mut relevant_events: Vec<RelevantEvent> = Vec::new();
            let mut player_stats: Vec<PlayerStats> = Vec::new();
            for player in match_log.players.iter() {
                player_stats.push(PlayerStats {
                    name: player.name.clone(),
                    auth: player.auth as usize,
                    caps: 0,
                    hold_start: None,
                    hold: 0,
                    returns: 0,
                    prevent_start: None,
                    prevent: 0,
                    ndps: 0,
                    pups: 0
                });
            }

            for (i, player) in match_log.players.iter().enumerate() {
                let player_events = EventsReader::new(player.events.clone())
                    .player_events(Team::from_usize(player.team).expect("Could not parse Team enum."), match_log.duration);

                // If the player is on a team at the start of the match, add a join event.
                if Team::from_usize(player.team).expect("Could not parse Team enum.") != Team::None {
                    relevant_events.push(RelevantEvent {
                        time: 0,
                        event_type: Event::Join,
                        player_index: i,
                        team: Team::from_usize(player.team).expect("Could not parse Team enum.")
                    });
                }

                // track relevent events
                for event in player_events {
                    match event.event_type {
                        Event::Capture => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Capture,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Join => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Join,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Quit => relevant_events.push(RelevantEvent {
                            time: event.time,
                            event_type: Event::Quit,
                            player_index: i,
                            team: event.team
                        }),
                        Event::Switch => {
                            relevant_events.push(RelevantEvent {
                                time: event.time,
                                event_type: Event::Join,
                                player_index: i,
                                team: event.team
                            });
                            relevant_events.push(RelevantEvent {
                                time: event.time,
                                event_type: Event::Quit,
                                player_index: i,
                                team: match event.team {
                                    Team::Red => Team::Blue,
                                    Team::Blue => Team::Red,
                                    _ => Team::None
                                }
                            });
                        },
                        _ => {}
                    };
                }
            }

            relevant_events.sort_unstable_by_key(|x| x.time);

            let mut red_team: Vec<usize> = Vec::new();
            let mut blue_team: Vec<usize> = Vec::new();
            let mut last_join_time: isize = -TIME_AFTER_JOIN_TO_IGNORE - RESPAWN_DURATION;
            let mut cap_diff: isize = 0;

            for event in relevant_events.iter() {
                match event.event_type {
                    // If a player joins, add them to the appropriate team and reset cap_diff and last_join_time
                    Event::Join => {
                        match event.team {
                            Team::Red => red_team.push(event.player_index),
                            Team::Blue => blue_team.push(event.player_index),
                            _ => {}
                        }
                        cap_diff = 0;
                        last_join_time = event.time as isize;
                    },
                    // If a player quits, check if the teams are full and the matchup's been going
                    // on long enough, and write the matchup to file if so. Then remove the leaver
                    // from their team. (We don't have to update the last_join_time cause teams
                    // aren't full, so we won't start tracking again till someone else joins.)
                    Event::Quit => {
                        if red_team.len() == 4 &&
                            blue_team.len() == 4 &&
                            event.time as isize > (last_join_time + MINIMUM_MATCHUP_LENGTH) {
                            write_matchup_without_stats(
                                &mut output_file, match_log.date, match_log.map_id, event.time - (last_join_time as usize), cap_diff,
                                &red_team, &blue_team, &player_stats
                            );
                        }
                        match event.team {
                            Team::Red => {red_team.retain(|&x| x != event.player_index);},
                            Team::Blue => {blue_team.retain(|&x| x != event.player_index);},
                            _ => {}
                        };
                    },
                    // If a player switches teams, remove them from their old team and add them to
                    // their new team. We don't have to update the last_join_time because if you
                    // can switch, that means the teams aren't full.
                    Event::Switch => {
                        match event.team {
                            Team::Red => {
                                red_team.retain(|&x| x != event.player_index);
                                blue_team.push(event.player_index);
                            },
                            Team::Blue => {
                                blue_team.retain(|&x| x != event.player_index);
                                red_team.push(event.player_index);
                            },
                            _ => {}
                        }
                    }
                    _ => {}
                }

                // If the event happened long enough after the last join time, update cap_diff if
                // it was a cap.
                if event.time as isize > (last_join_time + TIME_AFTER_JOIN_TO_IGNORE) {
                    match event.event_type {
                        Event::Capture => {
                            cap_diff += match event.team {
                                Team::Red => 1,
                                Team::Blue => -1,
                                _ => 0
                            };
                        },
                        _ => {}
                    }
                }
            }

            // At end of match, log the last matchup if teams are full and it's lasted long enough
            if red_team.len() == 4 &&
                blue_team.len() == 4 &&
                match_log.duration as isize > (last_join_time + MINIMUM_MATCHUP_LENGTH) {
                write_matchup_without_stats(
                    &mut output_file, match_log.date, match_log.map_id, match_log.duration - last_join_time as usize + RESPAWN_DURATION as usize, cap_diff,
                    &red_team, &blue_team, &player_stats
                );
            }
        }
    }
}

// Write matchup data, including player stats, to the output file.
// date, map_id, duration, cap_diff, then all player names
fn write_matchup_without_stats(
    output_file: &mut File,
    date: usize,
    map_id: usize,
    duration: usize,
    cap_diff: isize,
    red_team: &Vec<usize>,
    blue_team: &Vec<usize>,
    player_stats: &Vec<PlayerStats>
) {
    let mut cells: Vec<String> = vec![
        date.to_string(),
        map_id.to_string(),
        duration.to_string(),
        cap_diff.to_string()
    ];
    let current_players: Vec<usize> = vec![red_team.clone(), blue_team.clone()].concat();

    // add player names
    current_players.iter().for_each(|player| {
        cells.push(format!(
            "\"{}_{}\"",
            player_stats[*player].name.escape_default().to_string(),
            player_stats[*player].auth
        ));
    });
    output_file.write_all(
        format!(
            "\n{}",
            cells.join(",")
        ).as_ref()
    ).expect("Could not print matchup to file.");
}


// Write matchup data, including player stats, to the output file.
// date, map_id, duration, cap_diff, then all player names, then all their stats
fn write_matchup_with_stats(
    output_file: &mut File,
    date: usize,
    map_id: usize,
    duration: usize,
    cap_diff: isize,
    red_team: &Vec<usize>,
    blue_team: &Vec<usize>,
    player_stats: &Vec<PlayerStats>,
) {
    let mut cells: Vec<String> = vec![
        date.to_string(),
        map_id.to_string(),
        duration.to_string(),
        cap_diff.to_string()
    ];
    let current_players: Vec<usize> = vec![red_team.clone(), blue_team.clone()].concat();

    // add player names
    current_players.iter().for_each(|player| {
        cells.push(format!(
            "\"{}_{}\"",
            player_stats[*player].name.escape_default().to_string(),
            player_stats[*player].auth
        ));
    });
    // add player stats
    current_players.iter().for_each(|player| {
        cells.push(player_stats[*player].caps.to_string());
        cells.push(player_stats[*player].hold.to_string());
        cells.push(player_stats[*player].returns.to_string());
        cells.push(player_stats[*player].ndps.to_string());
        cells.push(player_stats[*player].pups.to_string());
    });
    output_file.write_all(
        format!(
            "\n{}",
            cells.join(",")
        ).as_ref()
    ).expect("Could not print matchup to file.");
}


// Write matchup data, including player stats, to the output file.
// date, map_id, duration, cap_diff, then all player names, then all their stats
fn write_ranked_matchup_with_stats(
    output_file: &mut File,
    date: usize,
    map_id: usize,
    duration: usize,
    cap_diff: isize,
    red_team: &Vec<usize>,
    blue_team: &Vec<usize>,
    player_stats: &Vec<PlayerStats>,
) {
    if red_team.len() != 4 || blue_team.len() != 4 {
        return;
    }

    let mut cells: Vec<String> = vec![
        date.to_string(),
        map_id.to_string(),
        duration.to_string(),
        cap_diff.to_string()
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
        cells.push(player_stats[*player].hold.to_string());
        cells.push(player_stats[*player].returns.to_string());
        cells.push(player_stats[*player].prevent.to_string());
        cells.push(player_stats[*player].ndps.to_string());
        cells.push(player_stats[*player].pups.to_string());
    });
    output_file.write_all(
        format!(
            "\n{}",
            cells.join(",")
        ).as_ref()
    ).expect("Could not print matchup to file.");
}
