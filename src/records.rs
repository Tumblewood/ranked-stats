use crate::events_reader::{Event, EventsReader, Team};
use crate::log_reader::{MatchIterator, MatchLog};
use num_traits::FromPrimitive;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

const MINIMUM_RANKED_MATCH_LENGTH: usize = 180 * 60;
const EIGHT_MINUTES: usize = 8 * 60 * 60; // 8 minutes in ticks (60 ticks per second)

#[derive(Debug, Clone)]
pub struct PlayerRecord {
    pub match_id: String,
    pub player_name: String,
    pub value: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PlayerGameStats {
    // Basic counting stats
    pub caps: usize,
    pub returns: usize,
    pub tags: usize,
    pub pops: usize,
    pub grabs: usize,
    pub pups: usize,
    pub quick_returns: usize,
    pub flaccid_grabs: usize,

    // Time-based stats (in ticks)
    pub hold: usize,
    pub prevent: usize,
    pub button: usize,

    // Tracking fields for ongoing activities
    pub hold_start: Option<usize>,
    pub prevent_start: Option<usize>,
    pub button_start: Option<usize>,
    pub last_grab_time: Option<usize>,
}

impl PlayerGameStats {
    fn finalize_time_stats(&mut self, end_time: usize, cutoff: usize) {
        // Finalize hold if still ongoing at cutoff
        if let Some(start) = self.hold_start {
            if start < cutoff {
                self.hold += cutoff.min(end_time) - start;
            }
        }

        // Finalize prevent if still ongoing at cutoff
        if let Some(start) = self.prevent_start {
            if start < cutoff {
                self.prevent += cutoff.min(end_time) - start;
            }
        }

        // Finalize button if still ongoing at cutoff
        if let Some(start) = self.button_start {
            if start < cutoff {
                self.button += cutoff.min(end_time) - start;
            }
        }
    }
}

#[derive(Default)]
struct StatLeaderboards {
    caps: BTreeMap<usize, Vec<(String, String)>>,
    returns: BTreeMap<usize, Vec<(String, String)>>,
    tags: BTreeMap<usize, Vec<(String, String)>>,
    pops: BTreeMap<usize, Vec<(String, String)>>,
    grabs: BTreeMap<usize, Vec<(String, String)>>,
    hold: BTreeMap<usize, Vec<(String, String)>>,
    prevent: BTreeMap<usize, Vec<(String, String)>>,
    button: BTreeMap<usize, Vec<(String, String)>>,
    pups: BTreeMap<usize, Vec<(String, String)>>,
    quick_returns: BTreeMap<usize, Vec<(String, String)>>,
    flaccid_grabs: BTreeMap<usize, Vec<(String, String)>>,
    tags_no_pops: BTreeMap<usize, Vec<(String, String)>>,
    returns_no_grabs: BTreeMap<usize, Vec<(String, String)>>,
    hold_no_returns: BTreeMap<usize, Vec<(String, String)>>,
    caps_no_returns: BTreeMap<usize, Vec<(String, String)>>,
}

pub struct RecordsCollector {
    full_all: StatLeaderboards,
    full_wins: StatLeaderboards,
    full_losses: StatLeaderboards,
    first8_all: StatLeaderboards,
    first8_wins: StatLeaderboards,
    first8_losses: StatLeaderboards,
}

impl RecordsCollector {
    pub fn new() -> Self {
        Self {
            full_all: StatLeaderboards::default(),
            full_wins: StatLeaderboards::default(),
            full_losses: StatLeaderboards::default(),
            first8_all: StatLeaderboards::default(),
            first8_wins: StatLeaderboards::default(),
            first8_losses: StatLeaderboards::default(),
        }
    }

    fn insert_record(map: &mut BTreeMap<usize, Vec<(String, String)>>, match_id: String, player_name: String, value: usize) {
        map.entry(value).or_insert_with(Vec::new).push((match_id, player_name));
    }

    fn insert_with_win_loss(
        all_board: &mut BTreeMap<usize, Vec<(String, String)>>,
        win_board: &mut BTreeMap<usize, Vec<(String, String)>>,
        loss_board: &mut BTreeMap<usize, Vec<(String, String)>>,
        match_id: String,
        player_name: String,
        value: usize,
        is_win: bool,
    ) {
        Self::insert_record(all_board, match_id.clone(), player_name.clone(), value);
        if is_win {
            Self::insert_record(win_board, match_id, player_name, value);
        } else {
            Self::insert_record(loss_board, match_id, player_name, value);
        }
    }

    pub fn process_match(&mut self, match_id: String, match_log: &MatchLog) {
        // Filter matches
        if !match_log.official
            || match_log.players.len() < 8
            || match_log.group != Some("".to_string())
            || match_log.time_limit != 8.0
            || match_log.duration < MINIMUM_RANKED_MATCH_LENGTH
        {
            return;
        }

        let mut player_full_stats: Vec<PlayerGameStats> = vec![PlayerGameStats::default(); match_log.players.len()];
        let mut player_first8_stats: Vec<PlayerGameStats> = vec![PlayerGameStats::default(); match_log.players.len()];

        // Collect all events from all players into a unified timeline for proper quick return tracking
        #[derive(Clone)]
        struct TimedEvent {
            time: usize,
            event_type: Event,
            player_idx: usize,
            team: Team,
        }

        let mut all_events = Vec::new();
        let mut all_first8_events = Vec::new();

        for (player_idx, player) in match_log.players.iter().enumerate() {
            let player_events = EventsReader::new(player.events.clone())
                .player_events(
                    Team::from_usize(player.team).expect("Could not parse Team enum."),
                    match_log.duration,
                );

            let team = Team::from_usize(player.team).expect("Could not parse Team enum.");

            for event in player_events.iter() {
                all_events.push(TimedEvent {
                    time: event.time,
                    event_type: event.event_type,
                    player_idx,
                    team,
                });

                if event.time <= EIGHT_MINUTES {
                    all_first8_events.push(TimedEvent {
                        time: event.time,
                        event_type: event.event_type,
                        player_idx,
                        team,
                    });
                }
            }
        }

        // Sort events by time for chronological processing
        all_events.sort_by_key(|e| e.time);
        all_first8_events.sort_by_key(|e| e.time);

        // Process full game events in chronological order
        let mut red_grab_time: Option<usize> = None;
        let mut blue_grab_time: Option<usize> = None;
        let mut cap_diff: isize = 0;

        for event in all_events.iter() {
            self.process_event(
                event.event_type,
                event.time,
                &mut player_full_stats[event.player_idx],
                &mut red_grab_time,
                &mut blue_grab_time,
                event.team,
                match_log.duration,
            );

            // Track cap_diff for win/loss determination
            if event.event_type == Event::Capture {
                match event.team {
                    Team::Red => cap_diff += 1,
                    Team::Blue => cap_diff -= 1,
                    _ => {}
                }
            }
        }

        // Process first 8 minutes events in chronological order
        let mut red_grab_time_first8: Option<usize> = None;
        let mut blue_grab_time_first8: Option<usize> = None;

        for event in all_first8_events.iter() {
            self.process_event(
                event.event_type,
                event.time,
                &mut player_first8_stats[event.player_idx],
                &mut red_grab_time_first8,
                &mut blue_grab_time_first8,
                event.team,
                EIGHT_MINUTES,
            );
        }

        // Finalize time-based stats for all players
        for player_idx in 0..match_log.players.len() {
            player_full_stats[player_idx].finalize_time_stats(match_log.duration, match_log.duration);
            player_first8_stats[player_idx].finalize_time_stats(match_log.duration, EIGHT_MINUTES);
        }

        // Insert records for each player
        for (player_idx, player) in match_log.players.iter().enumerate() {
            let player_name = player.name.clone();
            let player_team = Team::from_usize(player.team).expect("Could not parse Team enum.");

            // Determine if this player won
            let is_win = match player_team {
                Team::Red => cap_diff > 0,
                Team::Blue => cap_diff < 0,
                _ => false,
            };

            // Insert records for this player
            let full = &player_full_stats[player_idx];
            let first8 = &player_first8_stats[player_idx];

            // Basic stats - full game
            Self::insert_with_win_loss(&mut self.full_all.caps, &mut self.full_wins.caps, &mut self.full_losses.caps,
                match_id.clone(), player_name.clone(), full.caps, is_win);
            Self::insert_with_win_loss(&mut self.full_all.returns, &mut self.full_wins.returns, &mut self.full_losses.returns,
                match_id.clone(), player_name.clone(), full.returns, is_win);
            Self::insert_with_win_loss(&mut self.full_all.tags, &mut self.full_wins.tags, &mut self.full_losses.tags,
                match_id.clone(), player_name.clone(), full.tags, is_win);
            Self::insert_with_win_loss(&mut self.full_all.pops, &mut self.full_wins.pops, &mut self.full_losses.pops,
                match_id.clone(), player_name.clone(), full.pops, is_win);
            Self::insert_with_win_loss(&mut self.full_all.grabs, &mut self.full_wins.grabs, &mut self.full_losses.grabs,
                match_id.clone(), player_name.clone(), full.grabs, is_win);
            Self::insert_with_win_loss(&mut self.full_all.pups, &mut self.full_wins.pups, &mut self.full_losses.pups,
                match_id.clone(), player_name.clone(), full.pups, is_win);
            Self::insert_with_win_loss(&mut self.full_all.quick_returns, &mut self.full_wins.quick_returns, &mut self.full_losses.quick_returns,
                match_id.clone(), player_name.clone(), full.quick_returns, is_win);
            Self::insert_with_win_loss(&mut self.full_all.flaccid_grabs, &mut self.full_wins.flaccid_grabs, &mut self.full_losses.flaccid_grabs,
                match_id.clone(), player_name.clone(), full.flaccid_grabs, is_win);
            Self::insert_with_win_loss(&mut self.full_all.hold, &mut self.full_wins.hold, &mut self.full_losses.hold,
                match_id.clone(), player_name.clone(), full.hold / 60, is_win);
            Self::insert_with_win_loss(&mut self.full_all.prevent, &mut self.full_wins.prevent, &mut self.full_losses.prevent,
                match_id.clone(), player_name.clone(), full.prevent / 60, is_win);
            Self::insert_with_win_loss(&mut self.full_all.button, &mut self.full_wins.button, &mut self.full_losses.button,
                match_id.clone(), player_name.clone(), full.button / 60, is_win);

            // Conditional stats - full game
            if full.tags > 0 && full.pops == 0 {
                Self::insert_with_win_loss(&mut self.full_all.tags_no_pops, &mut self.full_wins.tags_no_pops, &mut self.full_losses.tags_no_pops,
                    match_id.clone(), player_name.clone(), full.tags, is_win);
            }
            if full.returns > 0 && full.grabs == 0 {
                Self::insert_with_win_loss(&mut self.full_all.returns_no_grabs, &mut self.full_wins.returns_no_grabs, &mut self.full_losses.returns_no_grabs,
                    match_id.clone(), player_name.clone(), full.returns, is_win);
            }
            if full.hold > 0 && full.returns == 0 {
                Self::insert_with_win_loss(&mut self.full_all.hold_no_returns, &mut self.full_wins.hold_no_returns, &mut self.full_losses.hold_no_returns,
                    match_id.clone(), player_name.clone(), full.hold / 60, is_win);
            }
            if full.caps > 0 && full.returns == 0 {
                Self::insert_with_win_loss(&mut self.full_all.caps_no_returns, &mut self.full_wins.caps_no_returns, &mut self.full_losses.caps_no_returns,
                    match_id.clone(), player_name.clone(), full.caps, is_win);
            }

            // First 8 minutes stats
            Self::insert_with_win_loss(&mut self.first8_all.caps, &mut self.first8_wins.caps, &mut self.first8_losses.caps,
                match_id.clone(), player_name.clone(), first8.caps, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.returns, &mut self.first8_wins.returns, &mut self.first8_losses.returns,
                match_id.clone(), player_name.clone(), first8.returns, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.tags, &mut self.first8_wins.tags, &mut self.first8_losses.tags,
                match_id.clone(), player_name.clone(), first8.tags, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.pops, &mut self.first8_wins.pops, &mut self.first8_losses.pops,
                match_id.clone(), player_name.clone(), first8.pops, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.grabs, &mut self.first8_wins.grabs, &mut self.first8_losses.grabs,
                match_id.clone(), player_name.clone(), first8.grabs, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.pups, &mut self.first8_wins.pups, &mut self.first8_losses.pups,
                match_id.clone(), player_name.clone(), first8.pups, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.quick_returns, &mut self.first8_wins.quick_returns, &mut self.first8_losses.quick_returns,
                match_id.clone(), player_name.clone(), first8.quick_returns, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.flaccid_grabs, &mut self.first8_wins.flaccid_grabs, &mut self.first8_losses.flaccid_grabs,
                match_id.clone(), player_name.clone(), first8.flaccid_grabs, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.hold, &mut self.first8_wins.hold, &mut self.first8_losses.hold,
                match_id.clone(), player_name.clone(), first8.hold / 60, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.prevent, &mut self.first8_wins.prevent, &mut self.first8_losses.prevent,
                match_id.clone(), player_name.clone(), first8.prevent / 60, is_win);
            Self::insert_with_win_loss(&mut self.first8_all.button, &mut self.first8_wins.button, &mut self.first8_losses.button,
                match_id.clone(), player_name.clone(), first8.button / 60, is_win);

            // Conditional stats - first 8 minutes
            if first8.tags > 0 && first8.pops == 0 {
                Self::insert_with_win_loss(&mut self.first8_all.tags_no_pops, &mut self.first8_wins.tags_no_pops, &mut self.first8_losses.tags_no_pops,
                    match_id.clone(), player_name.clone(), first8.tags, is_win);
            }
            if first8.returns > 0 && first8.grabs == 0 {
                Self::insert_with_win_loss(&mut self.first8_all.returns_no_grabs, &mut self.first8_wins.returns_no_grabs, &mut self.first8_losses.returns_no_grabs,
                    match_id.clone(), player_name.clone(), first8.returns, is_win);
            }
            if first8.hold > 0 && first8.returns == 0 {
                Self::insert_with_win_loss(&mut self.first8_all.hold_no_returns, &mut self.first8_wins.hold_no_returns, &mut self.first8_losses.hold_no_returns,
                    match_id.clone(), player_name.clone(), first8.hold / 60, is_win);
            }
            if first8.caps > 0 && first8.returns == 0 {
                Self::insert_with_win_loss(&mut self.first8_all.caps_no_returns, &mut self.first8_wins.caps_no_returns, &mut self.first8_losses.caps_no_returns,
                    match_id.clone(), player_name.clone(), first8.caps, is_win);
            }
        }
    }

    fn process_event(
        &self,
        event_type: Event,
        time: usize,
        stats: &mut PlayerGameStats,
        red_grab_time: &mut Option<usize>,
        blue_grab_time: &mut Option<usize>,
        team: Team,
        cutoff: usize,
    ) {
        // Don't process events after cutoff
        if time > cutoff {
            return;
        }

        match event_type {
            Event::Capture => {
                stats.caps += 1;
                stats.hold_start = None;

                // Clear team grab time
                match team {
                    Team::Red => *red_grab_time = None,
                    Team::Blue => *blue_grab_time = None,
                    _ => {}
                }
            }
            Event::Grab => {
                stats.grabs += 1;
                stats.hold_start = Some(time);
                stats.last_grab_time = Some(time);

                // Track team grab time for quick returns
                match team {
                    Team::Red => *red_grab_time = Some(time),
                    Team::Blue => *blue_grab_time = Some(time),
                    _ => {}
                }
            }
            Event::Drop => {
                stats.pops += 1; // Drops are also pops

                if let Some(hold_start) = stats.hold_start {
                    // Only count hold time up to cutoff
                    let end_time = time.min(cutoff);
                    if hold_start < cutoff {
                        stats.hold += end_time - hold_start;
                    }
                    stats.hold_start = None;
                }

                // Check for flaccid grab (drop within 2 seconds)
                if let Some(grab_time) = stats.last_grab_time {
                    if time > grab_time && time - grab_time < 2 * 60 { // 2 seconds in ticks
                        stats.flaccid_grabs += 1;
                    }
                }

                // Clear team grab time
                match team {
                    Team::Red => *red_grab_time = None,
                    Team::Blue => *blue_grab_time = None,
                    _ => {}
                }
            }
            Event::Return => {
                stats.returns += 1;
                stats.tags += 1; // Returns are also tags

                // Check for quick return (within 2 seconds of opponent grab)
                let opponent_grab_time = match team {
                    Team::Red => *blue_grab_time,
                    Team::Blue => *red_grab_time,
                    _ => None,
                };

                if let Some(grab_time) = opponent_grab_time {
                    if time > grab_time && time - grab_time < 2 * 60 { // 2 seconds in ticks
                        stats.quick_returns += 1;
                    }
                }
            }
            Event::Tag => {
                stats.tags += 1;
            }
            Event::Pop => {
                stats.pops += 1;
            }
            Event::Powerup | Event::DuplicatePowerup => {
                stats.pups += 1;
            }
            Event::StartPrevent => {
                stats.prevent_start = Some(time);
            }
            Event::StopPrevent => {
                if let Some(prevent_start) = stats.prevent_start {
                    // Only count prevent time up to cutoff
                    let end_time = time.min(cutoff);
                    if prevent_start < cutoff {
                        stats.prevent += end_time - prevent_start;
                    }
                    stats.prevent_start = None;
                }
            }
            Event::StartButton => {
                stats.button_start = Some(time);
            }
            Event::StopButton => {
                if let Some(button_start) = stats.button_start {
                    // Only count button time up to cutoff
                    let end_time = time.min(cutoff);
                    if button_start < cutoff {
                        stats.button += end_time - button_start;
                    }
                    stats.button_start = None;
                }
            }
            Event::Quit => {
                // Finalize any ongoing time stats
                if let Some(hold_start) = stats.hold_start {
                    let end_time = time.min(cutoff);
                    if hold_start < cutoff {
                        stats.hold += end_time - hold_start;
                    }
                    stats.hold_start = None;
                }
                if let Some(prevent_start) = stats.prevent_start {
                    let end_time = time.min(cutoff);
                    if prevent_start < cutoff {
                        stats.prevent += end_time - prevent_start;
                    }
                    stats.prevent_start = None;
                }
                if let Some(button_start) = stats.button_start {
                    let end_time = time.min(cutoff);
                    if button_start < cutoff {
                        stats.button += end_time - button_start;
                    }
                    stats.button_start = None;
                }
                // Don't count quit as a pop for records
            }
            _ => {}
        }
    }


    pub fn generate_report(&self, output_path: &str) {
        let mut file = File::create(output_path).expect("Could not create output file");

        writeln!(file, "=== ALL-TIME RANKED TAGPRO RECORDS ===\n").unwrap();

        // Full game records
        self.write_section(&mut file, "FULL GAME RECORDS (Including Overtime)", &self.full_all, &self.full_wins, &self.full_losses);

        // First 8 minutes records
        self.write_section(&mut file, "FIRST 8 MINUTES RECORDS", &self.first8_all, &self.first8_wins, &self.first8_losses);
    }

    fn write_section(&self, file: &mut File, title: &str, all: &StatLeaderboards, wins: &StatLeaderboards, losses: &StatLeaderboards) {
        writeln!(file, "## {}\n", title).unwrap();
        self.write_stat_group_merged(file, all, wins, losses);
    }

    fn write_stat_group_merged(&self, file: &mut File, all: &StatLeaderboards, wins: &StatLeaderboards, losses: &StatLeaderboards) {
        self.write_leaderboard_merged(file, "Captures", &all.caps, &wins.caps, &losses.caps);
        self.write_leaderboard_merged(file, "Returns", &all.returns, &wins.returns, &losses.returns);
        self.write_leaderboard_merged(file, "Tags", &all.tags, &wins.tags, &losses.tags);
        self.write_leaderboard_merged(file, "Pops", &all.pops, &wins.pops, &losses.pops);
        self.write_leaderboard_merged(file, "Grabs", &all.grabs, &wins.grabs, &losses.grabs);
        self.write_leaderboard_merged(file, "Hold (seconds)", &all.hold, &wins.hold, &losses.hold);
        self.write_leaderboard_merged(file, "Prevent (seconds)", &all.prevent, &wins.prevent, &losses.prevent);
        self.write_leaderboard_merged(file, "Button Time (seconds)", &all.button, &wins.button, &losses.button);
        self.write_leaderboard_merged(file, "Powerups", &all.pups, &wins.pups, &losses.pups);
        self.write_leaderboard_merged(file, "Quick Returns", &all.quick_returns, &wins.quick_returns, &losses.quick_returns);
        self.write_leaderboard_merged(file, "Flaccid Grabs", &all.flaccid_grabs, &wins.flaccid_grabs, &losses.flaccid_grabs);
        self.write_leaderboard_merged(file, "Tags (No Pops)", &all.tags_no_pops, &wins.tags_no_pops, &losses.tags_no_pops);
        self.write_leaderboard_merged(file, "Returns (No Grabs)", &all.returns_no_grabs, &wins.returns_no_grabs, &losses.returns_no_grabs);
        self.write_leaderboard_merged(file, "Hold (No Returns, seconds)", &all.hold_no_returns, &wins.hold_no_returns, &losses.hold_no_returns);
        self.write_leaderboard_merged(file, "Caps (No Returns)", &all.caps_no_returns, &wins.caps_no_returns, &losses.caps_no_returns);
    }

    fn get_top_n_with_status(map: &BTreeMap<usize, Vec<(String, String)>>, n: usize, is_win: bool) -> Vec<(String, String, usize, bool)> {
        let mut results = Vec::new();
        let mut current_rank = 1;

        for (&value, players) in map.iter().rev() {
            if value == 0 {
                continue; // Skip zero values
            }

            // Only include this value tier if the rank is within top N
            if current_rank > n {
                break;
            }

            for (match_id, player_name) in players {
                results.push((match_id.clone(), player_name.clone(), value, is_win));
            }

            // Next rank is current_rank + number of people at this value
            current_rank += players.len();
        }

        results
    }

    fn write_leaderboard_merged(
        &self,
        file: &mut File,
        title: &str,
        _all_map: &BTreeMap<usize, Vec<(String, String)>>,
        wins_map: &BTreeMap<usize, Vec<(String, String)>>,
        losses_map: &BTreeMap<usize, Vec<(String, String)>>
    ) {
        writeln!(file, "### {}", title).unwrap();

        // Get top 5 from wins and top 5 from losses separately
        let mut top_wins = Self::get_top_n_with_status(wins_map, 5, true);
        let mut top_losses = Self::get_top_n_with_status(losses_map, 5, false);

        // Merge the two lists
        let mut results = Vec::new();
        results.append(&mut top_wins);
        results.append(&mut top_losses);

        // Sort by value descending, then by player name
        results.sort_by(|a, b| {
            b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1))
        });

        if results.is_empty() {
            writeln!(file, "No records found.\n").unwrap();
            return;
        }

        for (match_id, player_name, value, is_win) in results {
            let status = if is_win { "Win" } else { "Loss" };
            writeln!(
                file,
                "  Match {}: {} - {} ({})",
                match_id, player_name, value, status
            ).unwrap();
        }
        writeln!(file).unwrap();
    }
}

pub fn collect_all_records(match_iterator: MatchIterator) {
    let mut collector = RecordsCollector::new();

    for (match_id, match_log) in match_iterator {
        collector.process_match(match_id, &match_log);
    }

    collector.generate_report("analysis/all_time_records.txt");
    println!("Records collected! Output written to analysis/all_time_records.txt");
}
