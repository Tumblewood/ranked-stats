#[allow(unused_imports)]
use num_traits::FromPrimitive;
use crate::log_reader::MatchIterator;
use crate::analysis_types::StatConfig;
use crate::event_processor::process_ranked_match;
use crate::ranked_analysis::RankedStatConfig;
use std::fs::File;
use std::io::Write;

const OUTPUT_PATH: &str = "analysis/matchups_with_stats.csv";

pub fn get_ranked_matchups(match_iterator: MatchIterator) {
    let mut output_file = File::create(OUTPUT_PATH)
        .unwrap_or(File::open(OUTPUT_PATH).expect("Could not open output file."));
    
    // Generate header automatically from the stat config
    let header = RankedStatConfig::generate_csv_header();
    output_file.write_all(header.as_bytes()).expect("Could not write header to file.");

    for (_match_id, match_log) in match_iterator {
        if let Some((result, player_names)) = process_ranked_match::<RankedStatConfig>(&match_log) {
            // Convert result to CSV row
            let csv_row = format!("\n{}", result.to_csv_row::<RankedStatConfig>(&player_names));
            output_file.write_all(csv_row.as_bytes()).expect("Could not write matchup to file.");
        }
    }
}

