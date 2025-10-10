#[allow(unused_imports)]
use num_traits::FromPrimitive;
use tagpro_eu_analysis::log_reader::MatchIterator;
use tagpro_eu_analysis::ranked_ratings::{get_ranked_matchups};

fn main() {
    let match_iterator = MatchIterator::new(394, 408);
    get_ranked_matchups(match_iterator);
}
