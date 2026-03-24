use crate::events_reader::{Event, EventsReader, Powerup, Team};
use crate::log_reader::{MatchIterator, MatchLog};
use num_traits::FromPrimitive;
use std::fs::File;
use std::io::{Write, Read};
use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct HoldOutcome {
    pub start_time: usize,
    pub end_time: usize,
    pub ended_in_cap: bool,
    pub end_x: Option<usize>,
    pub end_y: Option<usize>,
}

#[derive(Deserialize, Clone)]
struct MapData {
    width: usize,
    tiles: String, // base64-encoded map layout
}

// Load map data from bulkmaps.json
fn load_map_data() -> HashMap<usize, MapData> {
    let mut file = match File::open("data/bulkmaps.json") {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };

    let mut contents = String::new();
    if file.read_to_string(&mut contents).is_err() {
        return HashMap::new();
    }

    let maps: HashMap<String, MapData> = match serde_json::from_str(&contents) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };

    maps.into_iter()
        .filter_map(|(id, data)| id.parse::<usize>().ok().map(|id| (id, data)))
        .collect()
}

#[derive(Debug, Clone)]
pub struct GrabRecord {
    pub match_id: String,
    pub team: String,
    pub player_name: String,
    pub powerups: Vec<String>,
    pub opponents_preventing: usize,
    pub teammates_preventing: usize,
    pub current_hold: HoldOutcome,
    pub previous_team_hold: Option<HoldOutcome>,
    pub opponent_hold: Option<HoldOutcome>,
    pub next_cap_team: Option<String>,
    pub next_cap_time: Option<usize>,
}

impl GrabRecord {
    fn powerups_to_vec(powerup_bits: usize) -> Vec<String> {
        let mut pups = Vec::new();
        if powerup_bits & (Powerup::JukeJuice as usize) != 0 {
            pups.push("jj".to_string());
        }
        if powerup_bits & (Powerup::RollingBomb as usize) != 0 {
            pups.push("rb".to_string());
        }
        if powerup_bits & (Powerup::TagPro as usize) != 0 {
            pups.push("tp".to_string());
        }
        if powerup_bits & (Powerup::TopSpeed as usize) != 0 {
            pups.push("ts".to_string());
        }
        pups
    }

    pub fn to_csv_row(&self) -> String {
        let powerups_str = self.powerups.join("+");

        let previous_hold_str = match &self.previous_team_hold {
            Some(h) => format!("{},{},{},{},{}",
                h.start_time,
                h.end_time,
                if h.ended_in_cap { "1" } else { "0" },
                h.end_x.map(|x| x.to_string()).unwrap_or_else(|| "".to_string()),
                h.end_y.map(|y| y.to_string()).unwrap_or_else(|| "".to_string())
            ),
            None => ",,,,".to_string(),
        };

        let opponent_hold_str = match &self.opponent_hold {
            Some(h) => format!("{},{},{},{},{}",
                h.start_time,
                h.end_time,
                if h.ended_in_cap { "1" } else { "0" },
                h.end_x.map(|x| x.to_string()).unwrap_or_else(|| "".to_string()),
                h.end_y.map(|y| y.to_string()).unwrap_or_else(|| "".to_string())
            ),
            None => ",,,,".to_string(),
        };

        // Build row in parts since previous_hold_str and opponent_hold_str already contain commas
        let base_fields = format!(
            "{},{},\"{}\",\"{}\",{},{},{},{},{},{},{}",
            self.match_id,
            self.team,
            self.player_name,
            powerups_str,
            self.opponents_preventing,
            self.teammates_preventing,
            self.current_hold.start_time,
            self.current_hold.end_time,
            if self.current_hold.ended_in_cap { "1" } else { "0" },
            self.current_hold.end_x.map(|x| x.to_string()).unwrap_or_else(|| "".to_string()),
            self.current_hold.end_y.map(|y| y.to_string()).unwrap_or_else(|| "".to_string())
        );

        let next_cap_fields = format!(
            "{},{}",
            self.next_cap_team.as_ref().unwrap_or(&"".to_string()),
            self.next_cap_time.map(|t| t.to_string()).unwrap_or_else(|| "".to_string())
        );

        format!("{},{},{},{}", base_fields, previous_hold_str, opponent_hold_str, next_cap_fields)
    }

    pub fn csv_header() -> String {
        "match_id,team,player_name,powerups,opponents_preventing,teammates_preventing,\
        hold_start,hold_end,hold_capped,hold_end_x,hold_end_y,\
        prev_hold_start,prev_hold_end,prev_hold_capped,prev_hold_end_x,prev_hold_end_y,\
        opp_hold_start,opp_hold_end,opp_hold_capped,opp_hold_end_x,opp_hold_end_y,\
        next_cap_team,next_cap_time".to_string()
    }
}

#[derive(Debug, Clone)]
struct TimelineEvent {
    time: usize,
    event_type: Event,
    player_index: usize,
    team: Team,
    powerups: usize,
}

const OUTPUT_PATH: &str = "analysis/grab_stats.csv";

pub fn generate_grab_stats(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH)
        .unwrap_or(File::open(OUTPUT_PATH).expect("Could not open output file."));

    let header = GrabRecord::csv_header();
    output_file.write_all(header.as_bytes()).expect("Could not write header to file.");

    // Load map data once
    let map_data = load_map_data();
    println!("Loaded {} maps from bulkmaps.json", map_data.len());

    for (match_id, match_log) in match_iterator {
        if let Some(records) = process_match_for_grabs(match_id, &match_log, &map_data) {
            for record in records {
                let csv_row = format!("\n{}", record.to_csv_row());
                output_file.write_all(csv_row.as_bytes()).expect("Could not write grab record to file.");
            }
        }
    }
}

fn parse_splat_coordinates(
    splat_blob: &str,
    map_layout: &crate::events_reader::MapLayout,
    pop_times: &mut std::collections::BinaryHeap<std::cmp::Reverse<usize>>
) -> Vec<crate::events_reader::SplatEvent> {
    use crate::events_reader::{EventsReader, SplatEvent};
    use std::cmp::Reverse;

    let mut reader = EventsReader::new(splat_blob.to_string());
    let mut splats = Vec::new();

    // Calculate coordinate bits
    let x_bits = reader.bits_used_to_represent_coordinate(map_layout.width);
    let y_bits = reader.bits_used_to_represent_coordinate(map_layout.height);

    // Read splat coordinates and match with pop times
    while reader.events_remaining() {
        let num_splats = reader.read_tally();

        for _ in 0..num_splats {
            // Read coordinates
            let x_raw = reader.read_fixed(x_bits.0) as isize;
            let y_raw = reader.read_fixed(y_bits.0) as isize;
            let x = (x_raw - x_bits.1 as isize).max(0) as usize;
            let y = (y_raw - y_bits.1 as isize).max(0) as usize;

            // Match with the next pop time
            if let Some(Reverse(time)) = pop_times.pop() {
                splats.push(SplatEvent { x, y, time });
            }
        }
    }

    splats
}

fn process_match_for_grabs(match_id: String, match_log: &MatchLog, map_data_cache: &HashMap<usize, MapData>) -> Option<Vec<GrabRecord>> {
    // Filter for valid ranked CTF matches
    if !match_log.official
        || match_log.players.len() < 8
        || match_log.group != Some("".to_string())
        || match_log.time_limit != 8.0
    {
        return None;
    }

    // Parse splat coordinates and match them with player drop/pop events
    // The splat blob only contains coordinates - times come from player events!
    let (red_splats, blue_splats) = if let Some(map_data) = map_data_cache.get(&match_log.map_id) {
        use crate::events_reader::MapLayout;
        use std::collections::BinaryHeap;
        use std::cmp::Reverse;

        // Parse the actual height from the tiles data
        let height = std::panic::catch_unwind(|| {
            let layout = EventsReader::new(map_data.tiles.clone()).map_layout(map_data.width);
            layout.height
        }).unwrap_or(map_data.width); // Fallback to square map if parsing fails

        let map_layout = MapLayout {
            layout: Vec::new(),
            width: map_data.width,
            height,
        };

        // Build priority queues of drop/pop times for each team
        let mut red_pops: BinaryHeap<Reverse<usize>> = BinaryHeap::new();
        let mut blue_pops: BinaryHeap<Reverse<usize>> = BinaryHeap::new();

        for (_player_idx, player) in match_log.players.iter().enumerate() {
            let team = Team::from_usize(player.team).expect("Could not parse Team enum.");
            let player_events = EventsReader::new(player.events.clone())
                .player_events(team, match_log.duration);

            for event in player_events {
                if matches!(event.event_type, Event::Drop | Event::Pop) {
                    match team {
                        Team::Red => red_pops.push(Reverse(event.time)),
                        Team::Blue => blue_pops.push(Reverse(event.time)),
                        _ => {}
                    }
                }
            }
        }

        // Parse splat coordinates from blob and match with pop times
        let red_splats = parse_splat_coordinates(&match_log.teams[0].splats, &map_layout, &mut red_pops);
        let blue_splats = parse_splat_coordinates(&match_log.teams[1].splats, &map_layout, &mut blue_pops);

        (red_splats, blue_splats)
    } else {
        // No map data available for this map_id
        (Vec::new(), Vec::new())
    };


    // Collect all events from all players into a unified timeline
    let mut timeline: Vec<TimelineEvent> = Vec::new();
    let mut red_team: Vec<usize> = Vec::new();
    let mut blue_team: Vec<usize> = Vec::new();

    for (player_idx, player) in match_log.players.iter().enumerate() {
        let team = Team::from_usize(player.team).expect("Could not parse Team enum.");

        match team {
            Team::Red => red_team.push(player_idx),
            Team::Blue => blue_team.push(player_idx),
            _ => {}
        }

        let player_events = EventsReader::new(player.events.clone())
            .player_events(team, match_log.duration);

        for event in player_events {
            timeline.push(TimelineEvent {
                time: event.time,
                event_type: event.event_type,
                player_index: player_idx,
                team,
                powerups: event.powerups,
            });
        }
    }

    // Only process 4v4 matches
    if red_team.len() != 4 || blue_team.len() != 4 {
        return None;
    }

    // Sort timeline by time
    timeline.sort_by_key(|e| e.time);

    // Build hold tracking structures
    let mut holds: Vec<(usize, Team, HoldOutcome)> = Vec::new(); // (player_idx, team, outcome)
    let mut caps: Vec<(usize, Team)> = Vec::new(); // (time, team)
    let mut grab_prevent_counts: Vec<(usize, usize, usize)> = Vec::new(); // (grab_time, opponents_preventing, teammates_preventing)

    // Track active holds and prevent state
    let mut active_hold: Option<(usize, usize, Team, usize)> = None; // (player_idx, start_time, team, powerups)
    let mut preventing: Vec<bool> = vec![false; match_log.players.len()]; // Track prevent state per player

    for event in &timeline {
        match event.event_type {
            Event::Grab => {
                active_hold = Some((event.player_index, event.time, event.team, event.powerups));

                // Count opponents and teammates preventing at this grab time
                let opponent_team = match event.team {
                    Team::Red => Team::Blue,
                    Team::Blue => Team::Red,
                    _ => Team::None,
                };

                let opponent_indices: Vec<usize> = match opponent_team {
                    Team::Red => red_team.clone(),
                    Team::Blue => blue_team.clone(),
                    _ => Vec::new(),
                };

                let teammate_indices: Vec<usize> = match event.team {
                    Team::Red => red_team.clone(),
                    Team::Blue => blue_team.clone(),
                    _ => Vec::new(),
                };

                let opponents_preventing_count = opponent_indices.iter()
                    .filter(|&&idx| preventing[idx])
                    .count();

                let teammates_preventing_count = teammate_indices.iter()
                    .filter(|&&idx| preventing[idx] && idx != event.player_index) // Exclude the grabber
                    .count();

                grab_prevent_counts.push((event.time, opponents_preventing_count, teammates_preventing_count));
            }
            Event::StartPrevent => {
                preventing[event.player_index] = true;
            }
            Event::StopPrevent => {
                preventing[event.player_index] = false;
            }
            Event::Capture => {
                if let Some((player_idx, start_time, team, _)) = active_hold {
                    if player_idx == event.player_index {
                        holds.push((
                            player_idx,
                            team,
                            HoldOutcome {
                                start_time,
                                end_time: event.time,
                                ended_in_cap: true,
                                end_x: None,
                                end_y: None,
                            }
                        ));
                        caps.push((event.time, team));
                        active_hold = None;
                    }
                }
            }
            Event::Drop | Event::Pop => {
                if let Some((player_idx, start_time, team, _)) = active_hold {
                    if player_idx == event.player_index {
                        // Find splat location
                        let (end_x, end_y) = match team {
                            Team::Red => red_splats.iter()
                                .find(|s| s.time == event.time)
                                .map(|s| (Some(s.x), Some(s.y)))
                                .unwrap_or((None, None)),
                            Team::Blue => blue_splats.iter()
                                .find(|s| s.time == event.time)
                                .map(|s| (Some(s.x), Some(s.y)))
                                .unwrap_or((None, None)),
                            _ => (None, None),
                        };

                        holds.push((
                            player_idx,
                            team,
                            HoldOutcome {
                                start_time,
                                end_time: event.time,
                                ended_in_cap: false,
                                end_x,
                                end_y,
                            }
                        ));
                        active_hold = None;
                    }
                }
            }
            Event::Quit => {
                if let Some((player_idx, start_time, team, _)) = active_hold {
                    if player_idx == event.player_index {
                        holds.push((
                            player_idx,
                            team,
                            HoldOutcome {
                                start_time,
                                end_time: event.time,
                                ended_in_cap: false,
                                end_x: None,
                                end_y: None,
                            }
                        ));
                        active_hold = None;
                    }
                }
            }
            Event::End => {
                // Handle hold that was still active at game end
                if let Some((player_idx, start_time, team, _)) = active_hold {
                    holds.push((
                        player_idx,
                        team,
                        HoldOutcome {
                            start_time,
                            end_time: event.time,
                            ended_in_cap: false,
                            end_x: None,
                            end_y: None,
                        }
                    ));
                    active_hold = None;
                }
            }
            _ => {}
        }
    }

    // Now build GrabRecords for each grab event
    let mut grab_records = Vec::new();

    for grab_event in timeline.iter().filter(|e| e.event_type == Event::Grab) {
        let player_name = match_log.players[grab_event.player_index].name.clone();
        let powerups = GrabRecord::powerups_to_vec(grab_event.powerups);

        // Find this hold in our holds list
        let current_hold_idx = holds.iter()
            .position(|(idx, team, h)|
                *idx == grab_event.player_index &&
                *team == grab_event.team &&
                h.start_time == grab_event.time
            );

        if let Some(hold_idx) = current_hold_idx {
            let (_, _, current_hold) = &holds[hold_idx];

            // Find previous team hold (same team, earlier hold)
            let previous_team_hold = holds.iter()
                .rev()
                .skip(holds.len() - hold_idx)
                .find(|(_, team, h)| *team == grab_event.team && h.end_time < grab_event.time)
                .map(|(_, _, h)| h.clone());

            // Find opponent's current or next hold
            let opponent_team = match grab_event.team {
                Team::Red => Team::Blue,
                Team::Blue => Team::Red,
                _ => Team::None,
            };

            let opponent_hold = holds.iter()
                .find(|(_, team, h)|
                    *team == opponent_team &&
                    (h.start_time <= grab_event.time && h.end_time >= grab_event.time || // current
                     h.start_time > grab_event.time) // next
                )
                .map(|(_, _, h)| h.clone());

            // Find next cap
            let next_cap = caps.iter()
                .find(|(time, _)| *time > grab_event.time);

            let (next_cap_time, next_cap_team) = match next_cap {
                Some((time, team)) => (
                    Some(*time),
                    Some(match team {
                        Team::Red => "red".to_string(),
                        Team::Blue => "blue".to_string(),
                        _ => "".to_string(),
                    })
                ),
                None => (None, None),
            };

            // Find the prevent counts at this grab time
            let (opponents_preventing, teammates_preventing) = grab_prevent_counts.iter()
                .find(|(time, _, _)| *time == grab_event.time)
                .map(|(_, opp, tm)| (*opp, *tm))
                .unwrap_or((0, 0));

            let team_str = match grab_event.team {
                Team::Red => "red".to_string(),
                Team::Blue => "blue".to_string(),
                _ => "".to_string(),
            };

            grab_records.push(GrabRecord {
                match_id: match_id.clone(),
                team: team_str,
                player_name,
                powerups,
                opponents_preventing,
                teammates_preventing,
                current_hold: current_hold.clone(),
                previous_team_hold,
                opponent_hold,
                next_cap_team,
                next_cap_time,
            });
        }
    }

    Some(grab_records)
}
