#[allow(unused_imports)]
use num_traits::FromPrimitive;
use ranked_stats::log_reader::MatchIterator;
use ranked_stats::ranked_ratings::get_ranked_matchups;

fn main() {
    let match_iterator = MatchIterator::new(394, 417);
    get_ranked_matchups(match_iterator);
}
