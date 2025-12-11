use ranked_stats::log_reader::MatchIterator;
use ranked_stats::records::collect_cap_runs_and_comebacks;

fn main() {
    println!("Collecting cap runs and comebacks...");
    println!("Processing match files 394 through 413...\n");

    let match_iterator = MatchIterator::new(394, 414);
    collect_cap_runs_and_comebacks(match_iterator);
}
