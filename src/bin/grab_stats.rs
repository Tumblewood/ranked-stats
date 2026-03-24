use ranked_stats::log_reader::MatchIterator;
use ranked_stats::grab_analysis::generate_grab_stats;

fn main() {
    let match_iterator = MatchIterator::new(394, 419);
    generate_grab_stats(match_iterator);
}
