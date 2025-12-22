use ranked_stats::log_reader::MatchIterator;
use ranked_stats::records::{collect_all_records, collect_team_records, collect_combined_game_records};

fn main() {
    println!("Collecting all-time TagPro ranked records...");
    println!("Processing match files 394 through 413...\n");

    // Process all ranked match logs for player records
    let match_iterator = MatchIterator::new(394, 416);
    collect_all_records(match_iterator);

    println!();

    // Process all ranked match logs for team records
    let match_iterator2 = MatchIterator::new(394, 416);
    collect_team_records(match_iterator2);

    println!();

    // Process all ranked match logs for combined game records
    let match_iterator3 = MatchIterator::new(394, 416);
    collect_combined_game_records(match_iterator3);
}
