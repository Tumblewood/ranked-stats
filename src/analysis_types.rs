use crate::events_reader::{Event, Team};

#[derive(Debug, Clone)]
pub struct RelevantEvent {
    pub time: usize,
    pub event_type: Event,
    pub player_index: usize,
    pub team: Team,
}

#[derive(Debug, Clone)]
pub struct MatchResult<S> {
    pub match_id: String,
    pub timestamp: usize,
    pub map_id: usize,
    pub duration: usize,
    pub cap_diff: isize,
    pub garbage_time_cap_diff: isize,
    pub red_team: Vec<usize>,
    pub blue_team: Vec<usize>,
    pub player_stats: Vec<S>,
}

pub trait StatConfig {
    type Stats: Default + Clone;
    
    const RELEVANT_EVENTS: &'static [Event];
    const STAT_FIELDS: &'static [&'static str];
    
    fn process_event(
        event: &RelevantEvent,
        cap_diff: &mut isize,
        garbage_time_cap_diff: &mut isize,
        match_duration: usize,
        red_fc: &mut Option<usize>,
        blue_fc: &mut Option<usize>,
        red_grab_time: &mut Option<usize>,
        blue_grab_time: &mut Option<usize>,
        all_player_stats: &mut [Self::Stats],
    );
    
    fn post_process_stats(
        _all_events: &[RelevantEvent],
        _all_player_stats: &mut [Self::Stats],
        _red_team: &[usize],
        _blue_team: &[usize],
    ) {
        // Default implementation does nothing
    }
    
    fn to_csv_values(stats: &Self::Stats) -> Vec<String>;
    
    fn generate_csv_header() -> String {
        let mut header_parts = vec!["match_id", "map_id", "timestamp", "duration", "cap_diff", "garbage_time_cap_diff"];
        header_parts.extend(["r1", "r2", "r3", "r4", "b1", "b2", "b3", "b4"]);
        
        let mut stat_parts = Vec::new();
        for player in ["r1", "r2", "r3", "r4", "b1", "b2", "b3", "b4"] {
            for field in Self::STAT_FIELDS {
                stat_parts.push(format!("{}_{}", player, field));
            }
        }
        
        header_parts.extend(stat_parts.iter().map(|s| s.as_str()));
        header_parts.join(",")
    }
}

impl<S> MatchResult<S> {
    pub fn to_csv_row<C: StatConfig<Stats = S>>(&self, player_names: &[String]) -> String {
        let mut cells = vec![
            self.match_id.clone(),
            self.map_id.to_string(),
            self.timestamp.to_string(),
            self.duration.to_string(),
            self.cap_diff.to_string(),
            self.garbage_time_cap_diff.to_string(),
        ];
        
        // Add player names in red team then blue team order
        let current_players: Vec<usize> = [self.red_team.clone(), self.blue_team.clone()].concat();
        current_players.iter().for_each(|&player_idx| {
            cells.push(format!("\"{}\"", player_names[player_idx]));
        });
        
        // Add player stats
        current_players.iter().for_each(|&player_idx| {
            let stat_values = C::to_csv_values(&self.player_stats[player_idx]);
            cells.extend(stat_values);
        });
        
        cells.join(",")
    }
}