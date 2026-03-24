use crate::events_reader::{Event, EventsReader, Powerup, Team};
use crate::log_reader::MatchIterator;
use num_traits::FromPrimitive;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};

const OUTPUT_PATH: &str = "analysis/nf_pups.csv";

const TARGET_MAPS: &[&str] = &[
    "Plasma 2026",
    "Whiplash NFC",
    "Sky Dweller NFC",
    "Carrera NFC",
    "Bulldog 2023",
];

// Ticks per second is 60
const PUP_WINDOW_START: usize = 3 * 60;  // 3 seconds after collection
const PUP_WINDOW_END: usize = 23 * 60;   // 23 seconds after collection

const MINIMUM_RANKED_MATCH_LENGTH: usize = 2 * 60 * 60;

#[derive(Deserialize)]
struct BulkMapEntry {
    name: String,
}

fn load_map_names() -> HashMap<usize, String> {
    let mut file = match File::open("data/bulkmaps.json") {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };
    let mut contents = String::new();
    if file.read_to_string(&mut contents).is_err() {
        return HashMap::new();
    }
    let maps: HashMap<String, BulkMapEntry> = match serde_json::from_str(&contents) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };
    maps.into_iter()
        .filter_map(|(id, entry)| id.parse::<usize>().ok().map(|id| (id, entry.name)))
        .collect()
}

struct PupEvent {
    time: usize,
    team: Team,
    pup_type: &'static str, // "tp", "rb", or "jj"
}

struct CaptureEvent {
    time: usize,
    team: Team,
}

fn pup_to_str(powerup_bits: usize) -> Option<&'static str> {
    match Powerup::from_usize(powerup_bits) {
        Some(Powerup::TagPro) => Some("tp"),
        Some(Powerup::RollingBomb) => Some("rb"),
        Some(Powerup::JukeJuice) => Some("jj"),
        _ => None,
    }
}

pub fn collect_nf_pup_outcomes(match_iterator: MatchIterator) {
    let map_names = load_map_names();

    let mut output_file = File::create(OUTPUT_PATH).expect("Could not create output file.");
    output_file.write_all(b"map,pup,cf,ca").expect("Could not write header.");

    for (_match_id, match_log) in match_iterator {
        if !match_log.official
            || match_log.players.len() < 8
            || match_log.group != Some("".to_string())
            || match_log.time_limit != 8.0
            || match_log.duration < MINIMUM_RANKED_MATCH_LENGTH
        {
            continue;
        }

        let map_name = match map_names.get(&match_log.map_id) {
            Some(name) if TARGET_MAPS.contains(&name.as_str()) => name.clone(),
            _ => continue,
        };

        let duration = match_log.duration;
        let mut pup_events: Vec<PupEvent> = Vec::new();
        let mut capture_events: Vec<CaptureEvent> = Vec::new();

        for player in match_log.players.iter() {
            let team = match Team::from_usize(player.team) {
                Some(t) if t != Team::None => t,
                _ => continue,
            };

            let player_events = EventsReader::new(player.events.clone())
                .player_events(team, duration);

            let mut current_pups: usize = 0;

            for event in &player_events {
                match event.event_type {
                    Event::Powerup => {
                        // XOR reveals exactly which bit was gained
                        let gained = event.powerups ^ current_pups;
                        if let Some(pup_type) = pup_to_str(gained) {
                            pup_events.push(PupEvent {
                                time: event.time,
                                team: event.team,
                                pup_type,
                            });
                        }
                    }
                    Event::DuplicatePowerup => {
                        // Won't occur on these maps, but handle it by assuming
                        // the duplicate is the same type the player already has.
                        if let Some(pup_type) = pup_to_str(current_pups) {
                            pup_events.push(PupEvent {
                                time: event.time,
                                team: event.team,
                                pup_type,
                            });
                        }
                    }
                    Event::Capture => {
                        capture_events.push(CaptureEvent {
                            time: event.time,
                            team: event.team,
                        });
                    }
                    _ => {}
                }
                current_pups = event.powerups;
            }
        }

        for pup in &pup_events {
            let window_start = pup.time + PUP_WINDOW_START;
            let window_end = (pup.time + PUP_WINDOW_END).min(duration);

            let opponent_team = match pup.team {
                Team::Red => Team::Blue,
                Team::Blue => Team::Red,
                _ => continue,
            };

            let cf = capture_events.iter()
                .filter(|c| c.team == pup.team && c.time >= window_start && c.time <= window_end)
                .count();
            let ca = capture_events.iter()
                .filter(|c| c.team == opponent_team && c.time >= window_start && c.time <= window_end)
                .count();

            output_file.write_all(
                format!("\n\"{}\",{},{},{}", map_name, pup.pup_type, cf, ca).as_bytes()
            ).expect("Could not write row.");
        }
    }
}
