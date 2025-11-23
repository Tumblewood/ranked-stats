use ranked_stats::log_reader::MatchIterator;
use ranked_stats::records::collect_all_records;

fn main() {
    println!("Collecting all-time TagPro ranked records...");
    println!("Processing match files 394 through 413...\n");

    // Process all ranked match logs
    let match_iterator = MatchIterator::new(394, 414);
    collect_all_records(match_iterator);
}
