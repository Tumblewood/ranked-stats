pub struct Config {
    pub output_filename: String,
    pub start_index: isize,
    pub end_index: isize
}

pub fn parse_config(args: Vec<String>) -> Config {
    let start_index = match args.get(2) {
        Some(x) => x.parse::<isize>().unwrap_or(280),
        None => 280
    };
    let end_index = match args.get(3) {
        Some(x) => x.parse::<isize>().unwrap_or(281),
        None => 281
    };
    let output_filename = match args.get(1) {
        Some(x) => x.clone(),
        None => format!("out_{}_{}", start_index, end_index)
    };

    Config {
        output_filename,
        start_index,
        end_index
    }
}