use ranked_stats::log_reader::MatchIterator;
use ranked_stats::nf_pups::collect_nf_pup_outcomes;

fn main() {
    let match_iterator = MatchIterator::new(394, 419);
    collect_nf_pup_outcomes(match_iterator);
}
