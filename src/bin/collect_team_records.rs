use ranked_stats::log_reader::MatchIterator;
use ranked_stats::records::collect_team_records;

fn main() {
    println!("Collecting team records...");
    println!("Processing match files 394 through 413...\n");

    let match_iterator = MatchIterator::new(394, 414);
    collect_team_records(match_iterator);
}
