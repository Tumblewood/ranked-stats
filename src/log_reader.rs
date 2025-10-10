use std::collections::BTreeMap;
use std::collections::btree_map::IntoIter;
use std::option::Option::*;
use serde::Deserialize;
use std::io::Read;
use std::fs::File;

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct MatchLog {
    pub server: String,
    pub port: usize,
    pub official: bool,
    pub group: Option<String>,
    pub date: usize,
    pub time_limit: f32,
    pub duration: usize,
    pub finished: bool,
    pub map_id: usize,
    pub players: Vec<Player>,
    pub teams: [Team; 2]
}

#[derive(Clone, Deserialize, Debug)]
#[allow(dead_code)]
pub struct Player {
    pub auth: bool,
    pub name: String,
    pub flair: usize,
    pub degree: usize,
    pub score: isize,
    pub points: usize,
    pub team: usize,
    pub events: String
}

#[derive(Clone, Deserialize, Debug)]
#[allow(dead_code)]
pub struct Team {
    pub name: String,
    pub score: usize,
    pub splats: String
}

pub struct MatchIterator {
    log_file_index: usize,
    log_file_iterator: std::collections::btree_map::IntoIter<String, MatchLog>,
    end_index: usize
}

const DEFAULT_START_INDEX: usize = 394;
const DEFAULT_END_INDEX: usize = 403;

impl MatchIterator {
    pub fn new(start_index: usize, end_index: usize) -> MatchIterator {
        let mut log_file_index = start_index;
        let mut log_file_option = None;
        while log_file_index < DEFAULT_END_INDEX && log_file_option.is_none() {
            log_file_option = deserialize_log_file(
                format!("data/matches{}.json", log_file_index));
            log_file_index += 1;
        }

        MatchIterator {
            log_file_index,
            log_file_iterator: log_file_option.unwrap(),
            end_index
        }
    }
}

impl Default for MatchIterator {
    fn default() -> Self {
        Self::new(DEFAULT_START_INDEX, DEFAULT_END_INDEX)
    }
}

impl Iterator for MatchIterator {
    type Item = (String, MatchLog);

    fn next(&mut self) -> Option<Self::Item> {
        match self.log_file_iterator.next() {
            Some(i) => Some(i),
            None => {
                let mut log_file_option = None;
                while self.log_file_index < self.end_index && log_file_option.is_none() {
                    log_file_option = deserialize_log_file(
                        format!("data/matches{}.json", self.log_file_index));
                    self.log_file_index += 1;
                }
                self.log_file_iterator = log_file_option?;
                self.log_file_iterator.next()
            }
        }
    }
}

fn deserialize_log_file(filepath: String) -> Option<IntoIter<String, MatchLog>> {
    let mut s = String::new();
    File::open(&filepath).expect("Could not open matches file").read_to_string(&mut s).expect("Could not read matches file");
    // println!("Parsing {}", filepath);
    let match_logs: BTreeMap<String, MatchLog> = serde_json::from_str(&s).expect("Could not parse matches file");
    println!("{}", filepath);
    Some(match_logs.into_iter())
}