use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};

use crate::events_reader::{Event, EventsReader, Powerup, Team};
use crate::log_reader::{MatchIterator, MatchLog};
use num_traits::FromPrimitive;
use serde::Deserialize;

const OUTPUT_PATH: &str = "analysis/smurfs.csv";
const USERS_PATH: &str = "analysis/users.txt";

/// 60 ticks/s * 60 s = 3600 ticks per minute
const TICKS_PER_MINUTE: usize = 3600;

// ---------------------------------------------------------------------------
// Map type loading
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BulkMapEntry {
    #[serde(rename = "type")]
    map_type: String,
}

fn load_map_types() -> HashMap<usize, String> {
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
        .filter_map(|(id, entry)| id.parse::<usize>().ok().map(|id| (id, entry.map_type)))
        .collect()
}

// ---------------------------------------------------------------------------
// User list loading
// ---------------------------------------------------------------------------

fn load_users() -> Vec<String> {
    let file = match File::open(USERS_PATH) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("Warning: could not open {}", USERS_PATH);
            return Vec::new();
        }
    };
    BufReader::new(file)
        .lines()
        .filter_map(|l| l.ok())
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Game classification
// ---------------------------------------------------------------------------

enum GameType {
    RankedCtf,
    RankedNf,
    Casual,
    Other,
}

fn classify_game(match_log: &MatchLog, map_types: &HashMap<usize, String>) -> GameType {
    if match_log.official
        && match_log.players.len() >= 8
        && match_log.group == Some(String::new())
        && match_log.time_limit == 8.0
    {
        let mt = map_types.get(&match_log.map_id).map(|s| s.as_str()).unwrap_or("");
        return if mt == "nf" || mt == "2nf" {
            GameType::RankedNf
        } else {
            GameType::RankedCtf
        };
    }
    if match_log.group.is_none() && match_log.time_limit == 6.0 {
        return GameType::Casual;
    }
    GameType::Other
}

// ---------------------------------------------------------------------------
// Eastern time helpers (no external crates)
// ---------------------------------------------------------------------------

/// Howard Hinnant's civil_from_days.
/// Returns (year, month 1-12, day 1-31, weekday 0=Sun).
fn days_to_date(days: i64) -> (i32, u32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y } as i32;
    let weekday = ((days + 4).rem_euclid(7)) as u32; // Thu=4 on epoch
    (year, m, d, weekday)
}

/// US Eastern DST: starts second Sunday of March at 02:00, ends first Sunday
/// of November at 02:00.  `secs_of_day` is local time-of-day in EST (UTC-5).
fn is_dst(year: i32, month: u32, day: u32, weekday: u32, secs_of_day: u32) -> bool {
    let _ = year;
    if month < 3 || month > 11 {
        return false;
    }
    if month > 3 && month < 11 {
        return true;
    }
    let month1_wd = ((weekday as i32) - ((day as i32) - 1)).rem_euclid(7) as u32;
    if month == 3 {
        let first_sun = if month1_wd == 0 { 1 } else { 8 - month1_wd };
        let second_sun = first_sun + 7;
        if day < second_sun {
            return false;
        }
        if day > second_sun {
            return true;
        }
        return secs_of_day >= 2 * 3600;
    }
    // month == 11
    let first_sun = if month1_wd == 0 { 1 } else { 8 - month1_wd };
    if day < first_sun {
        return true;
    }
    if day > first_sun {
        return false;
    }
    secs_of_day < 2 * 3600
}

/// Returns (hour_0_23, day_of_week_0_6_sun, play_day_key).
/// play_day_key: day number in Eastern time, shifted back 6 h so that
/// 00:00-05:59 Eastern counts as the previous calendar day.
fn timestamp_to_ny(ts: usize) -> (u32, u32, i64) {
    // Approximate with EST (UTC-5) to determine DST
    let est_secs = ts as i64 - 5 * 3600;
    let est_days = est_secs.div_euclid(86400);
    let est_tod = est_secs.rem_euclid(86400) as u32;
    let (year, month, day, weekday) = days_to_date(est_days);
    let offset: i64 = if is_dst(year, month, day, weekday, est_tod) {
        4
    } else {
        5
    };

    let local_secs = ts as i64 - offset * 3600;
    let local_days = local_secs.div_euclid(86400);
    let local_tod = local_secs.rem_euclid(86400) as u32;
    let hour = local_tod / 3600;
    let (_, _, _, wd) = days_to_date(local_days);

    // Shift 6 h back so midnight–06:00 belongs to the previous day
    let shifted_secs = local_secs - 6 * 3600;
    let shifted_days = shifted_secs.div_euclid(86400);
    let (_, _, _, shifted_wd) = days_to_date(shifted_days);

    let _ = wd;
    (hour, shifted_wd, shifted_days)
}

// ---------------------------------------------------------------------------
// Per-player accumulator
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PlayerAccum {
    ranked_ctf_games: usize,
    ranked_nf_games: usize,
    casual_games: usize,

    // Time/day profiles (ranked CTF only)
    hour_counts: [usize; 24],
    day_counts: [usize; 7], // 0 = Sunday
    play_days: HashSet<i64>,

    // Grab difficulty
    grab_on_d: [usize; 5], // [0..=4 opponents preventing]

    // Time-on-task (ticks, ranked CTF only)
    total_hold_ticks: usize,
    total_prevent_ticks: usize,
    total_button_ticks: usize,
    total_block_ticks: usize,
    total_duration_ticks: usize,

    // Powerup stats (ranked CTF only)
    player_pups: usize,
    game_pups: usize,
    hold_at_pup: usize,
    tp_pups: usize,
    rb_pups: usize,
    jj_pups: usize,

    // Kiss stats (ranked CTF only)
    total_holds_ended: usize, // holds ending by cap, drop, quit (not "still active at End")
    kiss_holds: usize,

    // Disconnect (ranked CTF only)
    dc_games: usize,
}

// ---------------------------------------------------------------------------
// Timeline event (internal, in-memory only)
// ---------------------------------------------------------------------------

struct TL {
    time: usize,
    event_type: Event,
    player_index: usize,
    team: Team,
    powerups: usize,
}

// ---------------------------------------------------------------------------
// Per-game processing (single event-decode pass)
// ---------------------------------------------------------------------------

fn process_ranked_ctf_game(
    match_log: &MatchLog,
    target_player_indices: &[usize], // indices into match_log.players
    accum_indices: &[usize],         // parallel: which PlayerAccum to update
    accums: &mut [PlayerAccum],
    timestamp: usize,
) {
    let duration = match_log.duration;
    let n = target_player_indices.len();
    let num_players = match_log.players.len();

    // -----------------------------------------------------------------------
    // Decode events once and build a unified timeline
    // -----------------------------------------------------------------------
    let mut timeline: Vec<TL> = Vec::new();
    let mut red_team: Vec<usize> = Vec::new();
    let mut blue_team: Vec<usize> = Vec::new();

    for (pi, player) in match_log.players.iter().enumerate() {
        let team = match Team::from_usize(player.team) {
            Some(t) => t,
            None => continue,
        };
        match team {
            Team::Red => red_team.push(pi),
            Team::Blue => blue_team.push(pi),
            _ => {}
        }
        for ev in EventsReader::new(player.events.clone()).player_events(team, duration) {
            timeline.push(TL {
                time: ev.time,
                event_type: ev.event_type,
                player_index: pi,
                team: ev.team,
                powerups: ev.powerups,
            });
        }
    }

    timeline.sort_by_key(|e| e.time);

    // -----------------------------------------------------------------------
    // Pass 1 (in-memory): collect all FC-drop (time, team) pairs for kiss detection
    // -----------------------------------------------------------------------
    let mut fc_drop_times: HashSet<(usize, Team)> = HashSet::new();
    {
        let mut red_fc: Option<usize> = None;
        let mut blue_fc: Option<usize> = None;
        for ev in &timeline {
            let pi = ev.player_index;
            match ev.event_type {
                Event::Grab => match ev.team {
                    Team::Red => red_fc = Some(pi),
                    Team::Blue => blue_fc = Some(pi),
                    _ => {}
                },
                Event::Drop | Event::Pop => {
                    let is_fc = match ev.team {
                        Team::Red => red_fc == Some(pi),
                        Team::Blue => blue_fc == Some(pi),
                        _ => false,
                    };
                    if is_fc {
                        fc_drop_times.insert((ev.time, ev.team));
                        match ev.team {
                            Team::Red => red_fc = None,
                            Team::Blue => blue_fc = None,
                            _ => {}
                        }
                    }
                }
                Event::Capture | Event::Quit => match ev.team {
                    Team::Red => {
                        if red_fc == Some(pi) {
                            red_fc = None;
                        }
                    }
                    Team::Blue => {
                        if blue_fc == Some(pi) {
                            blue_fc = None;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pass 2 (in-memory): collect per-player stats
    // -----------------------------------------------------------------------

    let mut preventing = vec![false; num_players];
    let mut player_powerups = vec![0usize; num_players];

    // Per-target-player in-game state (indexed by ti)
    let mut hold_start: Vec<Option<usize>> = vec![None; n];
    let mut prevent_start: Vec<Option<usize>> = vec![None; n];
    let mut button_start: Vec<Option<usize>> = vec![None; n];
    let mut block_start: Vec<Option<usize>> = vec![None; n];
    let mut had_quit = vec![false; n];

    let mut game_pups = 0usize;

    for ev in &timeline {
        let pi = ev.player_index;

        // -- Global state updates (all players) ------------------------------

        match ev.event_type {
            Event::StartPrevent => preventing[pi] = true,
            Event::StopPrevent => preventing[pi] = false,
            _ => {}
        }

        match ev.event_type {
            Event::Powerup => {
                let gained = ev.powerups ^ player_powerups[pi];
                player_powerups[pi] = ev.powerups;
                game_pups += 1;

                for (ti, &tpi) in target_player_indices.iter().enumerate() {
                    let acc = &mut accums[accum_indices[ti]];
                    if hold_start[ti].is_some() {
                        acc.hold_at_pup += 1;
                    }
                    if tpi == pi {
                        acc.player_pups += 1;
                        match Powerup::from_usize(gained) {
                            Some(Powerup::TagPro) => acc.tp_pups += 1,
                            Some(Powerup::RollingBomb) => acc.rb_pups += 1,
                            Some(Powerup::JukeJuice) => acc.jj_pups += 1,
                            _ => {}
                        }
                    }
                }
            }
            Event::DuplicatePowerup => {
                // Gained type is the one the player already holds
                let existing = player_powerups[pi];
                game_pups += 1;

                for (ti, &tpi) in target_player_indices.iter().enumerate() {
                    let acc = &mut accums[accum_indices[ti]];
                    if hold_start[ti].is_some() {
                        acc.hold_at_pup += 1;
                    }
                    if tpi == pi {
                        acc.player_pups += 1;
                        match Powerup::from_usize(existing) {
                            Some(Powerup::TagPro) => acc.tp_pups += 1,
                            Some(Powerup::RollingBomb) => acc.rb_pups += 1,
                            Some(Powerup::JukeJuice) => acc.jj_pups += 1,
                            _ => {}
                        }
                    }
                }
            }
            Event::Powerdown => {
                player_powerups[pi] = ev.powerups;
            }
            _ => {}
        }

        // -- Per-target-player events ----------------------------------------

        let ti_opt = target_player_indices.iter().position(|&tpi| tpi == pi);
        if let Some(ti) = ti_opt {
            let acc = &mut accums[accum_indices[ti]];

            match ev.event_type {
                Event::Grab => {
                    hold_start[ti] = Some(ev.time);
                    // Count opponents preventing at grab time
                    let opp_team = match ev.team {
                        Team::Red => &blue_team,
                        Team::Blue => &red_team,
                        _ => &red_team,
                    };
                    let opp_cnt = opp_team
                        .iter()
                        .filter(|&&idx| preventing[idx])
                        .count()
                        .min(4);
                    acc.grab_on_d[opp_cnt] += 1;
                }
                Event::Capture => {
                    if let Some(start) = hold_start[ti].take() {
                        acc.total_hold_ticks += ev.time - start;
                        acc.total_holds_ended += 1;
                    }
                }
                Event::Drop | Event::Pop => {
                    if let Some(start) = hold_start[ti].take() {
                        acc.total_hold_ticks += ev.time - start;
                        acc.total_holds_ended += 1;
                        // Kiss: opponent FC also dropped at this tick
                        let opp_team = match ev.team {
                            Team::Red => Team::Blue,
                            Team::Blue => Team::Red,
                            _ => Team::None,
                        };
                        if fc_drop_times.contains(&(ev.time, opp_team)) {
                            acc.kiss_holds += 1;
                        }
                    }
                }
                Event::Quit => {
                    had_quit[ti] = true;
                    if let Some(start) = hold_start[ti].take() {
                        acc.total_hold_ticks += ev.time - start;
                        acc.total_holds_ended += 1;
                    }
                    if let Some(start) = prevent_start[ti].take() {
                        acc.total_prevent_ticks += ev.time - start;
                    }
                    if let Some(start) = button_start[ti].take() {
                        acc.total_button_ticks += ev.time - start;
                    }
                    if let Some(start) = block_start[ti].take() {
                        acc.total_block_ticks += ev.time - start;
                    }
                }
                Event::StartPrevent => prevent_start[ti] = Some(ev.time),
                Event::StopPrevent => {
                    if let Some(start) = prevent_start[ti].take() {
                        acc.total_prevent_ticks += ev.time - start;
                    }
                }
                Event::StartButton => button_start[ti] = Some(ev.time),
                Event::StopButton => {
                    if let Some(start) = button_start[ti].take() {
                        acc.total_button_ticks += ev.time - start;
                    }
                }
                Event::StartBlock => block_start[ti] = Some(ev.time),
                Event::StopBlock => {
                    if let Some(start) = block_start[ti].take() {
                        acc.total_block_ticks += ev.time - start;
                    }
                }
                Event::End => {
                    // Finalize any open intervals (hold not counted in total_holds_ended)
                    if let Some(start) = hold_start[ti].take() {
                        acc.total_hold_ticks += ev.time - start;
                    }
                    if let Some(start) = prevent_start[ti].take() {
                        acc.total_prevent_ticks += ev.time - start;
                    }
                    if let Some(start) = button_start[ti].take() {
                        acc.total_button_ticks += ev.time - start;
                    }
                    if let Some(start) = block_start[ti].take() {
                        acc.total_block_ticks += ev.time - start;
                    }
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Commit per-game totals to accumulators
    // -----------------------------------------------------------------------
    let (hour, shifted_wd, play_day_key) = timestamp_to_ny(timestamp);

    for ti in 0..n {
        let acc = &mut accums[accum_indices[ti]];
        acc.ranked_ctf_games += 1;
        acc.hour_counts[hour as usize] += 1;
        acc.day_counts[shifted_wd as usize] += 1;
        acc.play_days.insert(play_day_key);
        acc.total_duration_ticks += duration;
        acc.game_pups += game_pups;
        if had_quit[ti] {
            acc.dc_games += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

fn fmt_time_of_day(counts: &[usize; 24]) -> String {
    let total: usize = counts.iter().sum();
    if total == 0 {
        return "_".repeat(24);
    }
    counts
        .iter()
        .map(|&c| {
            let f = c as f64 / total as f64;
            if f > 0.20 {
                'O'
            } else if f > 0.10 {
                'o'
            } else if f > 0.03 {
                '.'
            } else {
                '_'
            }
        })
        .collect()
}

fn fmt_day_of_week(counts: &[usize; 7]) -> String {
    let total: usize = counts.iter().sum();
    if total == 0 {
        return "_".repeat(7);
    }
    counts
        .iter()
        .map(|&c| {
            let f = c as f64 / total as f64;
            if f > 0.25 {
                'O'
            } else if f > 0.15 {
                'o'
            } else if f > 0.05 {
                '.'
            } else {
                '_'
            }
        })
        .collect()
}

fn ratio(num: usize, den: usize) -> String {
    if den == 0 {
        return String::new();
    }
    format!("{:.3}", num as f64 / den as f64)
}

fn per_minute(ticks: usize, duration_ticks: usize) -> String {
    if duration_ticks == 0 {
        return String::new();
    }
    // ticks / 60 = seconds; / (duration_ticks / TICKS_PER_MINUTE) = per minute
    let seconds = ticks as f64 / 60.0;
    let minutes = duration_ticks as f64 / TICKS_PER_MINUTE as f64;
    format!("{:.3}", seconds / minutes)
}

fn write_row(name: &str, acc: &PlayerAccum) -> String {
    let total_grabs: usize = acc.grab_on_d.iter().sum();

    let avg_per_day = if acc.play_days.is_empty() {
        String::new()
    } else {
        format!(
            "{:.3}",
            acc.ranked_ctf_games as f64 / acc.play_days.len() as f64
        )
    };

    let dc_pct = if acc.ranked_ctf_games == 0 {
        String::new()
    } else {
        format!(
            "{:.3}",
            acc.dc_games as f64 / acc.ranked_ctf_games as f64
        )
    };

    format!(
        "\"{}\",{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        name,
        acc.ranked_ctf_games,
        acc.ranked_nf_games,
        acc.casual_games,
        fmt_time_of_day(&acc.hour_counts),
        fmt_day_of_week(&acc.day_counts),
        avg_per_day,
        ratio(acc.grab_on_d[0], total_grabs),
        ratio(acc.grab_on_d[1], total_grabs),
        ratio(acc.grab_on_d[2], total_grabs),
        ratio(acc.grab_on_d[3], total_grabs),
        ratio(acc.grab_on_d[4], total_grabs),
        per_minute(acc.total_hold_ticks, acc.total_duration_ticks),
        per_minute(acc.total_prevent_ticks, acc.total_duration_ticks),
        ratio(acc.player_pups, acc.game_pups),
        ratio(acc.hold_at_pup, acc.game_pups),
        ratio(acc.tp_pups, acc.player_pups),
        ratio(acc.rb_pups, acc.player_pups),
        ratio(acc.jj_pups, acc.player_pups),
        ratio(acc.kiss_holds, acc.total_holds_ended),
        per_minute(acc.total_button_ticks, acc.total_duration_ticks),
        per_minute(acc.total_block_ticks, acc.total_duration_ticks),
        dc_pct,
    )
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn collect_smurf_stats(match_iterator: MatchIterator) {
    let users = load_users();
    if users.is_empty() {
        eprintln!("No users found in {}", USERS_PATH);
        return;
    }

    let map_types = load_map_types();
    let mut accums: Vec<PlayerAccum> = users.iter().map(|_| PlayerAccum::default()).collect();

    for (_match_id, match_log) in match_iterator {
        match classify_game(&match_log, &map_types) {
            GameType::RankedCtf => {
                // Find which target users appear in this game
                let mut target_player_indices: Vec<usize> = Vec::new();
                let mut accum_indices: Vec<usize> = Vec::new();
                for (ui, user) in users.iter().enumerate() {
                    if let Some(pi) = match_log.players.iter().position(|p| &p.name == user) {
                        target_player_indices.push(pi);
                        accum_indices.push(ui);
                    }
                }
                if target_player_indices.is_empty() {
                    continue;
                }
                let timestamp = match_log.date;
                process_ranked_ctf_game(
                    &match_log,
                    &target_player_indices,
                    &accum_indices,
                    &mut accums,
                    timestamp,
                );
            }
            GameType::RankedNf => {
                for (ui, user) in users.iter().enumerate() {
                    if match_log.players.iter().any(|p| &p.name == user) {
                        accums[ui].ranked_nf_games += 1;
                    }
                }
            }
            GameType::Casual => {
                for (ui, user) in users.iter().enumerate() {
                    if match_log.players.iter().any(|p| &p.name == user) {
                        accums[ui].casual_games += 1;
                    }
                }
            }
            GameType::Other => {}
        }
    }

    // Write CSV
    let mut output = File::create(OUTPUT_PATH).expect("Could not create smurfs output file");
    let header = "player,ranked_ctf_games,ranked_nf_games,casual_games,\
        time_of_day,day_of_week,avg_per_day,\
        grab_on_0d,grab_on_1d,grab_on_2d,grab_on_3d,grab_on_4d,\
        hold_pm,prevent_pm,pup_pct,hold_during_pups,\
        tp_pct,rb_pct,jj_pct,kiss_pct,\
        button_per_minute,block_per_minute,dc_pct";
    output.write_all(header.as_bytes()).expect("Could not write header");

    for (user, acc) in users.iter().zip(accums.iter()) {
        let row = format!("\n{}", write_row(user, acc));
        output
            .write_all(row.as_bytes())
            .expect("Could not write row");
    }
}
