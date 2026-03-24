use ranked_stats::log_reader::MatchIterator;
use ranked_stats::smurfs::collect_smurf_stats;

fn main() {
    let match_iterator = MatchIterator::new(394, 423);
    collect_smurf_stats(match_iterator);
}
