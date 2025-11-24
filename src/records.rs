use crate::events_reader::{Event, EventsReader, Team};
use crate::log_reader::{MatchIterator, MatchLog};
use num_traits::FromPrimitive;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

const MINIMUM_RANKED_MATCH_LENGTH: usize = 180 * 60;
const MINIMUM_RECORD_MATCH_LENGTH: usize = 90 * 60; // 90 seconds in ticks (60 ticks per second)
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
            || match_log.duration < MINIMUM_RECORD_MATCH_LENGTH  // Skip games under 90 seconds
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

            // Find the player's Join event to determine their actual team
            let player_events = EventsReader::new(player.events.clone())
                .player_events(
                    Team::from_usize(player.team).expect("Could not parse Team enum."),
                    match_log.duration,
                );

            let player_team = player_events.iter()
                .find(|e| e.event_type == Event::Join)
                .map(|e| e.team)
                .unwrap_or(Team::from_usize(player.team).expect("Could not parse Team enum."));

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
        Self::process_event_static(event_type, time, stats, red_grab_time, blue_grab_time, team, cutoff);
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

#[derive(Default)]
struct TeamLeaderboards {
    // Highs
    caps: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    tags: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    returns: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    hold: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    prevent: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    pups: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    quick_returns: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    non_tag_pops: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    hold_differential: BTreeMap<isize, Vec<(String, Vec<String>)>>,

    // Lows (for select stats)
    tags_low: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    returns_low: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    hold_low: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    prevent_low: BTreeMap<usize, Vec<(String, Vec<String>)>>,
    pups_low: BTreeMap<usize, Vec<(String, Vec<String>)>>,
}

pub struct TeamRecordsCollector {
    full_wins: TeamLeaderboards,
    full_losses: TeamLeaderboards,
    first8_wins: TeamLeaderboards,
    first8_losses: TeamLeaderboards,
}

impl TeamRecordsCollector {
    pub fn new() -> Self {
        Self {
            full_wins: TeamLeaderboards::default(),
            full_losses: TeamLeaderboards::default(),
            first8_wins: TeamLeaderboards::default(),
            first8_losses: TeamLeaderboards::default(),
        }
    }

    fn insert_team_record(map: &mut BTreeMap<usize, Vec<(String, Vec<String>)>>, match_id: String, team_players: Vec<String>, value: usize) {
        map.entry(value).or_insert_with(Vec::new).push((match_id, team_players));
    }

    fn insert_team_record_signed(map: &mut BTreeMap<isize, Vec<(String, Vec<String>)>>, match_id: String, team_players: Vec<String>, value: isize) {
        map.entry(value).or_insert_with(Vec::new).push((match_id, team_players));
    }

    pub fn process_match(&mut self, match_id: String, match_log: &MatchLog) {
        // Filter matches
        if !match_log.official
            || match_log.players.len() < 8
            || match_log.group != Some("".to_string())
            || match_log.time_limit != 8.0
            || match_log.duration < MINIMUM_RANKED_MATCH_LENGTH
            || match_log.duration < MINIMUM_RECORD_MATCH_LENGTH  // Skip games under 90 seconds
        {
            return;
        }

        let mut player_full_stats: Vec<PlayerGameStats> = vec![PlayerGameStats::default(); match_log.players.len()];
        let mut player_first8_stats: Vec<PlayerGameStats> = vec![PlayerGameStats::default(); match_log.players.len()];

        // Collect all events
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

        all_events.sort_by_key(|e| e.time);
        all_first8_events.sort_by_key(|e| e.time);

        // Process full game events
        let mut red_grab_time: Option<usize> = None;
        let mut blue_grab_time: Option<usize> = None;
        let mut cap_diff: isize = 0;

        for event in all_events.iter() {
            RecordsCollector::process_event_static(
                event.event_type,
                event.time,
                &mut player_full_stats[event.player_idx],
                &mut red_grab_time,
                &mut blue_grab_time,
                event.team,
                match_log.duration,
            );

            if event.event_type == Event::Capture {
                match event.team {
                    Team::Red => cap_diff += 1,
                    Team::Blue => cap_diff -= 1,
                    _ => {}
                }
            }
        }

        // Process first 8 minutes events
        let mut red_grab_time_first8: Option<usize> = None;
        let mut blue_grab_time_first8: Option<usize> = None;

        for event in all_first8_events.iter() {
            RecordsCollector::process_event_static(
                event.event_type,
                event.time,
                &mut player_first8_stats[event.player_idx],
                &mut red_grab_time_first8,
                &mut blue_grab_time_first8,
                event.team,
                EIGHT_MINUTES,
            );
        }

        // Finalize time-based stats
        for player_idx in 0..match_log.players.len() {
            player_full_stats[player_idx].finalize_time_stats(match_log.duration, match_log.duration);
            player_first8_stats[player_idx].finalize_time_stats(match_log.duration, EIGHT_MINUTES);
        }

        // Aggregate team stats - use Join event to determine actual team
        let mut red_team_players = Vec::new();
        let mut blue_team_players = Vec::new();
        let mut red_full_stats = PlayerGameStats::default();
        let mut blue_full_stats = PlayerGameStats::default();
        let mut red_first8_stats = PlayerGameStats::default();
        let mut blue_first8_stats = PlayerGameStats::default();

        for (player_idx, player) in match_log.players.iter().enumerate() {
            // Find the player's Join event to determine their actual team
            let player_events = EventsReader::new(player.events.clone())
                .player_events(
                    Team::from_usize(player.team).expect("Could not parse Team enum."),
                    match_log.duration,
                );

            // Find Join event
            let player_team = player_events.iter()
                .find(|e| e.event_type == Event::Join)
                .map(|e| e.team)
                .unwrap_or(Team::from_usize(player.team).expect("Could not parse Team enum."));

            match player_team {
                Team::Red => {
                    red_team_players.push(player.name.clone());
                    Self::add_stats(&mut red_full_stats, &player_full_stats[player_idx]);
                    Self::add_stats(&mut red_first8_stats, &player_first8_stats[player_idx]);
                }
                Team::Blue => {
                    blue_team_players.push(player.name.clone());
                    Self::add_stats(&mut blue_full_stats, &player_full_stats[player_idx]);
                    Self::add_stats(&mut blue_first8_stats, &player_first8_stats[player_idx]);
                }
                _ => {}
            }
        }

        // Insert team records
        let red_wins = cap_diff > 0;
        let blue_wins = cap_diff < 0;

        // Full game records - Red team
        Self::insert_team_stats(&mut self.full_wins, &mut self.full_losses,
            match_id.clone(), red_team_players.clone(), &red_full_stats, &blue_full_stats, red_wins);

        // Full game records - Blue team
        Self::insert_team_stats(&mut self.full_wins, &mut self.full_losses,
            match_id.clone(), blue_team_players.clone(), &blue_full_stats, &red_full_stats, blue_wins);

        // First 8 minutes records - Red team
        Self::insert_team_stats(&mut self.first8_wins, &mut self.first8_losses,
            match_id.clone(), red_team_players.clone(), &red_first8_stats, &blue_first8_stats, red_wins);

        // First 8 minutes records - Blue team
        Self::insert_team_stats(&mut self.first8_wins, &mut self.first8_losses,
            match_id, blue_team_players, &blue_first8_stats, &red_first8_stats, blue_wins);
    }

    fn add_stats(total: &mut PlayerGameStats, player: &PlayerGameStats) {
        total.caps += player.caps;
        total.returns += player.returns;
        total.tags += player.tags;
        total.pops += player.pops;
        total.grabs += player.grabs;
        total.pups += player.pups;
        total.quick_returns += player.quick_returns;
        total.hold += player.hold;
        total.prevent += player.prevent;
    }

    fn insert_team_stats(
        wins: &mut TeamLeaderboards,
        losses: &mut TeamLeaderboards,
        match_id: String,
        team_players: Vec<String>,
        team_stats: &PlayerGameStats,
        opponent_stats: &PlayerGameStats,
        is_win: bool,
    ) {
        let board = if is_win { wins } else { losses };

        // Highs
        Self::insert_team_record(&mut board.caps, match_id.clone(), team_players.clone(), team_stats.caps);
        Self::insert_team_record(&mut board.tags, match_id.clone(), team_players.clone(), team_stats.tags);
        Self::insert_team_record(&mut board.returns, match_id.clone(), team_players.clone(), team_stats.returns);
        Self::insert_team_record(&mut board.hold, match_id.clone(), team_players.clone(), team_stats.hold / 60);
        Self::insert_team_record(&mut board.prevent, match_id.clone(), team_players.clone(), team_stats.prevent / 60);
        Self::insert_team_record(&mut board.pups, match_id.clone(), team_players.clone(), team_stats.pups);
        Self::insert_team_record(&mut board.quick_returns, match_id.clone(), team_players.clone(), team_stats.quick_returns);

        // Non-tag pops: team pops minus opponent tags
        let non_tag_pops = team_stats.pops.saturating_sub(opponent_stats.tags);
        Self::insert_team_record(&mut board.non_tag_pops, match_id.clone(), team_players.clone(), non_tag_pops);

        // Hold differential: team hold minus opponent hold (in seconds)
        let hold_diff = (team_stats.hold as isize - opponent_stats.hold as isize) / 60;
        Self::insert_team_record_signed(&mut board.hold_differential, match_id.clone(), team_players.clone(), hold_diff);

        // Lows (for select stats)
        Self::insert_team_record(&mut board.tags_low, match_id.clone(), team_players.clone(), team_stats.tags);
        Self::insert_team_record(&mut board.returns_low, match_id.clone(), team_players.clone(), team_stats.returns);
        Self::insert_team_record(&mut board.hold_low, match_id.clone(), team_players.clone(), team_stats.hold / 60);
        Self::insert_team_record(&mut board.prevent_low, match_id.clone(), team_players.clone(), team_stats.prevent / 60);
        Self::insert_team_record(&mut board.pups_low, match_id, team_players, team_stats.pups);
    }

    pub fn generate_report(&self, output_path: &str) {
        let mut file = File::create(output_path).expect("Could not create output file");

        writeln!(file, "=== ALL-TIME RANKED TAGPRO TEAM RECORDS ===\n").unwrap();

        // Full game records
        self.write_section(&mut file, "FULL GAME RECORDS (Including Overtime)", &self.full_wins, &self.full_losses);

        // First 8 minutes records
        self.write_section(&mut file, "FIRST 8 MINUTES RECORDS", &self.first8_wins, &self.first8_losses);
    }

    fn write_section(&self, file: &mut File, title: &str, wins: &TeamLeaderboards, losses: &TeamLeaderboards) {
        writeln!(file, "## {}\n", title).unwrap();

        writeln!(file, "### HIGHS\n").unwrap();
        self.write_team_leaderboard(file, "Captures", &wins.caps, &losses.caps);
        self.write_team_leaderboard(file, "Tags", &wins.tags, &losses.tags);
        self.write_team_leaderboard(file, "Returns", &wins.returns, &losses.returns);
        self.write_team_leaderboard(file, "Hold (seconds)", &wins.hold, &losses.hold);
        self.write_team_leaderboard(file, "Prevent (seconds)", &wins.prevent, &losses.prevent);
        self.write_team_leaderboard(file, "Powerups", &wins.pups, &losses.pups);
        self.write_team_leaderboard(file, "Quick Returns", &wins.quick_returns, &losses.quick_returns);
        self.write_team_leaderboard(file, "Non-Tag Pops", &wins.non_tag_pops, &losses.non_tag_pops);
        self.write_team_leaderboard_signed(file, "Hold Differential (seconds)", &wins.hold_differential, &losses.hold_differential);

        writeln!(file, "\n### LOWS\n").unwrap();
        self.write_team_leaderboard_low(file, "Tags", &wins.tags_low, &losses.tags_low);
        self.write_team_leaderboard_low(file, "Returns", &wins.returns_low, &losses.returns_low);
        self.write_team_leaderboard_low(file, "Hold (seconds)", &wins.hold_low, &losses.hold_low);
        self.write_team_leaderboard_low(file, "Prevent (seconds)", &wins.prevent_low, &losses.prevent_low);
        self.write_team_leaderboard_low(file, "Powerups", &wins.pups_low, &losses.pups_low);
    }

    fn get_top_n_teams(map: &BTreeMap<usize, Vec<(String, Vec<String>)>>, n: usize, is_win: bool) -> Vec<(String, Vec<String>, usize, bool)> {
        let mut results = Vec::new();
        let mut current_rank = 1;

        for (&value, teams) in map.iter().rev() {
            if value == 0 {
                continue;
            }

            if current_rank > n {
                break;
            }

            for (match_id, team_players) in teams {
                results.push((match_id.clone(), team_players.clone(), value, is_win));
            }

            current_rank += teams.len();
        }

        results
    }

    fn get_bottom_n_teams(map: &BTreeMap<usize, Vec<(String, Vec<String>)>>, n: usize, is_win: bool) -> Vec<(String, Vec<String>, usize, bool)> {
        let mut results = Vec::new();
        let mut current_rank = 1;

        for (&value, teams) in map.iter() {  // Iterate forward for lowest values
            if current_rank > n {
                break;
            }

            for (match_id, team_players) in teams {
                results.push((match_id.clone(), team_players.clone(), value, is_win));
            }

            current_rank += teams.len();
        }

        results
    }

    fn get_top_n_teams_signed(map: &BTreeMap<isize, Vec<(String, Vec<String>)>>, n: usize, is_win: bool) -> Vec<(String, Vec<String>, isize, bool)> {
        let mut results = Vec::new();
        let mut current_rank = 1;

        for (&value, teams) in map.iter().rev() {
            if current_rank > n {
                break;
            }

            for (match_id, team_players) in teams {
                results.push((match_id.clone(), team_players.clone(), value, is_win));
            }

            current_rank += teams.len();
        }

        results
    }

    fn write_team_leaderboard(
        &self,
        file: &mut File,
        title: &str,
        wins_map: &BTreeMap<usize, Vec<(String, Vec<String>)>>,
        losses_map: &BTreeMap<usize, Vec<(String, Vec<String>)>>
    ) {
        writeln!(file, "#### {}", title).unwrap();

        let mut top_wins = Self::get_top_n_teams(wins_map, 5, true);
        let mut top_losses = Self::get_top_n_teams(losses_map, 5, false);

        let mut results = Vec::new();
        results.append(&mut top_wins);
        results.append(&mut top_losses);

        results.sort_by(|a, b| {
            b.2.cmp(&a.2).then_with(|| a.1[0].cmp(&b.1[0]))
        });

        if results.is_empty() {
            writeln!(file, "No records found.\n").unwrap();
            return;
        }

        for (match_id, team_players, value, is_win) in results {
            let status = if is_win { "Win" } else { "Loss" };
            let players_str = team_players.join(", ");
            writeln!(
                file,
                "  Match {}: {} - {} ({})",
                match_id, players_str, value, status
            ).unwrap();
        }
        writeln!(file).unwrap();
    }

    fn write_team_leaderboard_low(
        &self,
        file: &mut File,
        title: &str,
        wins_map: &BTreeMap<usize, Vec<(String, Vec<String>)>>,
        losses_map: &BTreeMap<usize, Vec<(String, Vec<String>)>>
    ) {
        writeln!(file, "#### {}", title).unwrap();

        let mut bottom_wins = Self::get_bottom_n_teams(wins_map, 5, true);
        let mut bottom_losses = Self::get_bottom_n_teams(losses_map, 5, false);

        let mut results = Vec::new();
        results.append(&mut bottom_wins);
        results.append(&mut bottom_losses);

        results.sort_by(|a, b| {
            a.2.cmp(&b.2).then_with(|| a.1[0].cmp(&b.1[0]))  // Sort ascending for lows
        });

        if results.is_empty() {
            writeln!(file, "No records found.\n").unwrap();
            return;
        }

        for (match_id, team_players, value, is_win) in results {
            let status = if is_win { "Win" } else { "Loss" };
            let players_str = team_players.join(", ");
            writeln!(
                file,
                "  Match {}: {} - {} ({})",
                match_id, players_str, value, status
            ).unwrap();
        }
        writeln!(file).unwrap();
    }

    fn write_team_leaderboard_signed(
        &self,
        file: &mut File,
        title: &str,
        wins_map: &BTreeMap<isize, Vec<(String, Vec<String>)>>,
        losses_map: &BTreeMap<isize, Vec<(String, Vec<String>)>>
    ) {
        writeln!(file, "#### {}", title).unwrap();

        let mut top_wins = Self::get_top_n_teams_signed(wins_map, 5, true);
        let mut top_losses = Self::get_top_n_teams_signed(losses_map, 5, false);

        let mut results = Vec::new();
        results.append(&mut top_wins);
        results.append(&mut top_losses);

        results.sort_by(|a, b| {
            b.2.cmp(&a.2).then_with(|| a.1[0].cmp(&b.1[0]))
        });

        if results.is_empty() {
            writeln!(file, "No records found.\n").unwrap();
            return;
        }

        for (match_id, team_players, value, is_win) in results {
            let status = if is_win { "Win" } else { "Loss" };
            let players_str = team_players.join(", ");
            writeln!(
                file,
                "  Match {}: {} - {} ({})",
                match_id, players_str, value, status
            ).unwrap();
        }
        writeln!(file).unwrap();
    }
}

impl RecordsCollector {
    fn process_event_static(
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

                // Count hold time up to the capture
                if let Some(hold_start) = stats.hold_start {
                    let end_time = time.min(cutoff);
                    if hold_start < cutoff {
                        stats.hold += end_time - hold_start;
                    }
                    stats.hold_start = None;
                }

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

                match team {
                    Team::Red => *red_grab_time = Some(time),
                    Team::Blue => *blue_grab_time = Some(time),
                    _ => {}
                }
            }
            Event::Drop => {
                stats.pops += 1;

                if let Some(hold_start) = stats.hold_start {
                    let end_time = time.min(cutoff);
                    if hold_start < cutoff {
                        stats.hold += end_time - hold_start;
                    }
                    stats.hold_start = None;
                }

                if let Some(grab_time) = stats.last_grab_time {
                    if time > grab_time && time - grab_time < 2 * 60 {
                        stats.flaccid_grabs += 1;
                    }
                }

                match team {
                    Team::Red => *red_grab_time = None,
                    Team::Blue => *blue_grab_time = None,
                    _ => {}
                }
            }
            Event::Return => {
                stats.returns += 1;
                stats.tags += 1;

                let opponent_grab_time = match team {
                    Team::Red => *blue_grab_time,
                    Team::Blue => *red_grab_time,
                    _ => None,
                };

                if let Some(grab_time) = opponent_grab_time {
                    if time > grab_time && time - grab_time < 2 * 60 {
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
                    let end_time = time.min(cutoff);
                    if button_start < cutoff {
                        stats.button += end_time - button_start;
                    }
                    stats.button_start = None;
                }
            }
            Event::Quit => {
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
            }
            _ => {}
        }
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

#[derive(Default)]
struct CombinedGameLeaderboards {
    // Highs
    tags: BTreeMap<usize, Vec<String>>,
    returns: BTreeMap<usize, Vec<String>>,
    hold: BTreeMap<usize, Vec<String>>,
    prevent: BTreeMap<usize, Vec<String>>,
    quick_returns: BTreeMap<usize, Vec<String>>,
    non_tag_pops: BTreeMap<usize, Vec<String>>,

    // Lows (excluding non-tag pops)
    tags_low: BTreeMap<usize, Vec<String>>,
    returns_low: BTreeMap<usize, Vec<String>>,
    hold_low: BTreeMap<usize, Vec<String>>,
    prevent_low: BTreeMap<usize, Vec<String>>,
    quick_returns_low: BTreeMap<usize, Vec<String>>,
}

pub struct CombinedGameRecordsCollector {
    full: CombinedGameLeaderboards,
    first8: CombinedGameLeaderboards,
}

impl CombinedGameRecordsCollector {
    pub fn new() -> Self {
        Self {
            full: CombinedGameLeaderboards::default(),
            first8: CombinedGameLeaderboards::default(),
        }
    }

    fn insert_game_record(map: &mut BTreeMap<usize, Vec<String>>, match_id: String, value: usize) {
        map.entry(value).or_insert_with(Vec::new).push(match_id);
    }

    pub fn process_match(&mut self, match_id: String, match_log: &MatchLog) {
        // Filter matches
        if !match_log.official
            || match_log.players.len() < 8
            || match_log.group != Some("".to_string())
            || match_log.time_limit != 8.0
            || match_log.duration < MINIMUM_RANKED_MATCH_LENGTH
            || match_log.duration < MINIMUM_RECORD_MATCH_LENGTH  // Skip games under 90 seconds
        {
            return;
        }

        let mut player_full_stats: Vec<PlayerGameStats> = vec![PlayerGameStats::default(); match_log.players.len()];
        let mut player_first8_stats: Vec<PlayerGameStats> = vec![PlayerGameStats::default(); match_log.players.len()];

        // Collect all events
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

        all_events.sort_by_key(|e| e.time);
        all_first8_events.sort_by_key(|e| e.time);

        // Process full game events
        let mut red_grab_time: Option<usize> = None;
        let mut blue_grab_time: Option<usize> = None;

        for event in all_events.iter() {
            RecordsCollector::process_event_static(
                event.event_type,
                event.time,
                &mut player_full_stats[event.player_idx],
                &mut red_grab_time,
                &mut blue_grab_time,
                event.team,
                match_log.duration,
            );
        }

        // Process first 8 minutes events
        let mut red_grab_time_first8: Option<usize> = None;
        let mut blue_grab_time_first8: Option<usize> = None;

        for event in all_first8_events.iter() {
            RecordsCollector::process_event_static(
                event.event_type,
                event.time,
                &mut player_first8_stats[event.player_idx],
                &mut red_grab_time_first8,
                &mut blue_grab_time_first8,
                event.team,
                EIGHT_MINUTES,
            );
        }

        // Finalize time-based stats
        for player_idx in 0..match_log.players.len() {
            player_full_stats[player_idx].finalize_time_stats(match_log.duration, match_log.duration);
            player_first8_stats[player_idx].finalize_time_stats(match_log.duration, EIGHT_MINUTES);
        }

        // Aggregate combined stats (all players across both teams)
        let mut combined_full_stats = PlayerGameStats::default();
        let mut combined_first8_stats = PlayerGameStats::default();

        for player_idx in 0..match_log.players.len() {
            TeamRecordsCollector::add_stats(&mut combined_full_stats, &player_full_stats[player_idx]);
            TeamRecordsCollector::add_stats(&mut combined_first8_stats, &player_first8_stats[player_idx]);
        }

        // Insert records
        Self::insert_combined_stats(&mut self.full, match_id.clone(), &combined_full_stats);
        Self::insert_combined_stats(&mut self.first8, match_id, &combined_first8_stats);
    }

    fn insert_combined_stats(
        board: &mut CombinedGameLeaderboards,
        match_id: String,
        stats: &PlayerGameStats,
    ) {
        // Highs
        Self::insert_game_record(&mut board.tags, match_id.clone(), stats.tags);
        Self::insert_game_record(&mut board.returns, match_id.clone(), stats.returns);
        Self::insert_game_record(&mut board.hold, match_id.clone(), stats.hold / 60);
        Self::insert_game_record(&mut board.prevent, match_id.clone(), stats.prevent / 60);
        Self::insert_game_record(&mut board.quick_returns, match_id.clone(), stats.quick_returns);

        // Non-tag pops: For combined, this doesn't make sense the same way, but we can calculate it as
        // total pops across both teams. Actually, non-tag pops should be pops that weren't from tags.
        // Since both teams combined: combined_pops - combined_tags doesn't make sense.
        // Let me think... if we want non-tag pops for the game, it would be pops that happened
        // due to spikes, OOB, etc. But we're aggregating both teams, so it's just total pops - total tags from opponent.
        // Actually for a single team it's: team_pops - opponent_tags
        // For combined game, we'd want: (red_pops - blue_tags) + (blue_pops - red_tags)
        // But we don't have team separation here anymore.
        // I think the user wants total non-tag pops across the game, which would be situations where
        // the flag was dropped without a tag. But in our data model, we only track individual stats.
        // Let me re-read the requirement... "non-tag pops" for combined game.
        // I think it might just be: total pops that weren't from a Return event.
        // But our pops include both Drop and Pop events, and tags include both Return and Tag events.
        // Returns are a subset of tags. Pops can come from Returns (which are tags).
        // So non-tag pops would be: pops that happened without a corresponding tag/return.
        // For the game total, I think we can just use: total_pops - total_returns
        // Since returns always result in a pop, and non-return pops are drops/self-pops.
        let non_tag_pops = stats.pops.saturating_sub(stats.returns);
        Self::insert_game_record(&mut board.non_tag_pops, match_id.clone(), non_tag_pops);

        // Lows
        Self::insert_game_record(&mut board.tags_low, match_id.clone(), stats.tags);
        Self::insert_game_record(&mut board.returns_low, match_id.clone(), stats.returns);
        Self::insert_game_record(&mut board.hold_low, match_id.clone(), stats.hold / 60);
        Self::insert_game_record(&mut board.prevent_low, match_id.clone(), stats.prevent / 60);
        Self::insert_game_record(&mut board.quick_returns_low, match_id, stats.quick_returns);
    }

    pub fn generate_report(&self, output_path: &str) {
        let mut file = File::create(output_path).expect("Could not create output file");

        writeln!(file, "=== ALL-TIME RANKED TAGPRO COMBINED GAME RECORDS ===\n").unwrap();

        // Full game records
        self.write_section(&mut file, "FULL GAME RECORDS (Including Overtime)", &self.full);

        // First 8 minutes records
        self.write_section(&mut file, "FIRST 8 MINUTES RECORDS", &self.first8);
    }

    fn write_section(&self, file: &mut File, title: &str, board: &CombinedGameLeaderboards) {
        writeln!(file, "## {}\n", title).unwrap();

        writeln!(file, "### HIGHS\n").unwrap();
        self.write_game_leaderboard(file, "Tags", &board.tags);
        self.write_game_leaderboard(file, "Returns", &board.returns);
        self.write_game_leaderboard(file, "Hold (seconds)", &board.hold);
        self.write_game_leaderboard(file, "Prevent (seconds)", &board.prevent);
        self.write_game_leaderboard(file, "Quick Returns", &board.quick_returns);
        self.write_game_leaderboard(file, "Non-Tag Pops", &board.non_tag_pops);

        writeln!(file, "\n### LOWS\n").unwrap();
        self.write_game_leaderboard_low(file, "Tags", &board.tags_low);
        self.write_game_leaderboard_low(file, "Returns", &board.returns_low);
        self.write_game_leaderboard_low(file, "Hold (seconds)", &board.hold_low);
        self.write_game_leaderboard_low(file, "Prevent (seconds)", &board.prevent_low);
        self.write_game_leaderboard_low(file, "Quick Returns", &board.quick_returns_low);
    }

    fn get_top_n_games(map: &BTreeMap<usize, Vec<String>>, n: usize) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let mut current_rank = 1;

        for (&value, match_ids) in map.iter().rev() {
            if value == 0 {
                continue;
            }

            if current_rank > n {
                break;
            }

            for match_id in match_ids {
                results.push((match_id.clone(), value));
            }

            current_rank += match_ids.len();
        }

        results
    }

    fn get_bottom_n_games(map: &BTreeMap<usize, Vec<String>>, n: usize) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let mut current_rank = 1;

        for (&value, match_ids) in map.iter() {
            if current_rank > n {
                break;
            }

            for match_id in match_ids {
                results.push((match_id.clone(), value));
            }

            current_rank += match_ids.len();
        }

        results
    }

    fn write_game_leaderboard(&self, file: &mut File, title: &str, map: &BTreeMap<usize, Vec<String>>) {
        writeln!(file, "#### {}", title).unwrap();

        let results = Self::get_top_n_games(map, 5);

        if results.is_empty() {
            writeln!(file, "No records found.\n").unwrap();
            return;
        }

        for (match_id, value) in results {
            writeln!(file, "  Match {}: {}", match_id, value).unwrap();
        }
        writeln!(file).unwrap();
    }

    fn write_game_leaderboard_low(&self, file: &mut File, title: &str, map: &BTreeMap<usize, Vec<String>>) {
        writeln!(file, "#### {}", title).unwrap();

        let results = Self::get_bottom_n_games(map, 5);

        if results.is_empty() {
            writeln!(file, "No records found.\n").unwrap();
            return;
        }

        for (match_id, value) in results {
            writeln!(file, "  Match {}: {}", match_id, value).unwrap();
        }
        writeln!(file).unwrap();
    }
}

pub fn collect_team_records(match_iterator: MatchIterator) {
    let mut collector = TeamRecordsCollector::new();

    for (match_id, match_log) in match_iterator {
        collector.process_match(match_id, &match_log);
    }

    collector.generate_report("analysis/team_records.txt");
    println!("Team records collected! Output written to analysis/team_records.txt");
}

pub fn collect_combined_game_records(match_iterator: MatchIterator) {
    let mut collector = CombinedGameRecordsCollector::new();

    for (match_id, match_log) in match_iterator {
        collector.process_match(match_id, &match_log);
    }

    collector.generate_report("analysis/combined_game_records.txt");
    println!("Combined game records collected! Output written to analysis/combined_game_records.txt");
}
