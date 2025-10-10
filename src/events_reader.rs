use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use base64::Engine;

#[derive(Clone, Copy, PartialEq, Eq, Debug, FromPrimitive)]
pub enum Team {
    None = 0,
    Red = 1,
    Blue = 2
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, FromPrimitive)]
pub enum Flag {
    None = 0,
    Opponent = 1,
    OpponentPotato = 2,
    Neutral = 3,
    NeutralPotato = 4,
    Temporary = 5
}

#[derive(Clone, Copy, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum Powerup {
    None = 0,
    JukeJuice = 1,
    RollingBomb = 2,
    TagPro = 4,
    TopSpeed = 8
}

#[derive(Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum MapTile {
    Empty = 0,
    Floor = 20,
    RedTeamTile = 110,
    BlueTeamTile = 120,
    YellowTeamTile = 230,
    Wall = 10,
    LowerLeftWall = 11,
    UpperLeftWall = 12,
    UpperRightWall = 13,
    LowerRightWall = 14,
    RedFlag = 30,
    BlueFlag = 40,
    NeutralFlag = 160,
    TemporaryFlag = 161,
    RedEndZone = 170,
    BlueEndZone = 180,
    NeutralBoost = 50,
    RedBoost = 140,
    BlueBoost = 150,
    Powerup = 60,
    JukeJuice = 61,
    RollingBomb = 62,
    TagPro = 63,
    TopSpeed = 64,
    Spike = 70,
    Bomb = 100,
    GravityWell = 220,
    Button = 80,
    GrayGate = 90,
    GreenGate = 91,
    RedGate = 92,
    BlueGate = 93,
    EntryPortal = 130,
    ExitPortal = 131,
    RedPotato = 190,
    BluePotato = 200,
    NeutralPotato = 210,
    MarsBall = 211
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Event {
    Join,
    Quit,
    Switch,
    End,
    Grab,
    Capture,
    FlaglessCapture,
    Powerup,
    DuplicatePowerup,
    Powerdown,
    Return,
    Tag,
    Drop,
    Pop,
    StartPrevent,
    StopPrevent,
    StartButton,
    StopButton,
    StartBlock,
    StopBlock
}

#[derive(Debug)]
pub struct PlayerEvent {
    pub event_type: Event,
    pub time: usize,
    pub flag: Flag,
    pub powerups: usize,
    pub team: Team
}

pub struct EventsReader {
    data: Vec<u8>,
    pos: usize
}

#[allow(dead_code)]
pub struct MapLayout {
    pub layout: Vec<MapTile>,
    pub width: usize,
    pub height: usize
}

#[allow(dead_code)]
impl MapLayout {
    fn tile_at(&self, x: usize, y: usize) -> MapTile {
        self.layout[x + y * self.width]
    }
}

#[allow(dead_code)]
pub struct SplatEvent {
    pub x: usize,
    pub y: usize,
    pub time: usize
}

impl EventsReader {
    pub fn new(b64_data: String) -> EventsReader {
        EventsReader {
            data: base64::engine::general_purpose::STANDARD.decode(b64_data).unwrap(),
            pos: 0
        }
    }

    fn events_remaining(&self) -> bool {
        (self.pos >> 3) < self.data.len()
    }

    fn read_bool(&mut self) -> bool {
        match self.events_remaining() {
            true => {
                let result = (self.data[self.pos >> 3] >> (7 - (self.pos & 7))) & 1;
                self.pos += 1;
                result == 1
            }
            false => false
        }
    }

    fn read_fixed(&mut self, num_bits: usize) -> usize {
        let mut result = 0;
        for _ in 0..num_bits {
            result = result << 1 | (self.read_bool() as usize);
        }
        result
    }

    fn read_tally(&mut self) -> usize {
        let mut result = 0;
        while self.read_bool() {
            result += 1;
        }
        result
    }

    fn read_footer(&mut self) -> usize {
        let mut size = self.read_fixed(2) << 3;
        let mut free = (8 - (self.pos & 7)) & 7;
        size |= free;
        let mut minimum = 0;
        while free < size {
            minimum += 1 << free;
            free += 8;
        }
        (self.read_fixed(size) + minimum) as usize
    }

    pub fn player_events(&mut self, mut team: Team, duration: usize) -> Vec<PlayerEvent> {
        let mut time: usize = 0;
        let mut flag = Flag::None;
        let mut powerups: usize = 0;
        let mut preventing = false;
        let mut buttoning = false;
        let mut blocking = false;

        self.pos = 0;
        let mut events: Vec<PlayerEvent> = Vec::new();

        while self.events_remaining() {
            let new_team = if self.read_bool() {
                match (team, self.read_bool()) {
                    (Team::None, false) => Team::Red,
                    (Team::None, true) => Team::Blue,
                    (Team::Red, false) => Team::Blue,
                    (Team::Blue, false) => Team::Red,
                    _ => Team::None
                }
            } else { team };

            let pop_occurred = self.read_bool();
            let num_returns = self.read_tally();
            let num_tags = self.read_tally();
            let grab_occurred = (flag == Flag::None) && self.read_bool();
            let mut num_captures = self.read_tally();

            let mut flag_kept = !pop_occurred && new_team != Team::None &&
                (num_captures == 0 || (flag == Flag::None && !grab_occurred) || self.read_bool());
            let new_flag = if grab_occurred {
                match flag_kept {
                    true => Flag::from_usize(1 + self.read_fixed(2)).unwrap(),
                    false => Flag::Temporary
                }
            } else { flag };

            let mut num_new_powerups = self.read_tally();
            let mut powerups_gained: usize = 0;
            let mut powerups_lost: usize = 0;
            let mut i: usize = 1;
            while i < 16 {
                if (powerups & i) != 0 {
                    if self.read_bool() {
                        powerups_lost |= i;
                    }
                } else if num_new_powerups != 0 && self.read_bool() {
                    powerups_gained |= i;
                    num_new_powerups -= 1;
                }
                i <<= 1;
            }

            let toggle_preventing = self.read_bool();
            let toggle_buttoning = self.read_bool();
            let toggle_blocking = self.read_bool();
            time += 1 + self.read_footer();

            if team == Team::None && new_team != Team::None {
                team = new_team;
                events.push(PlayerEvent{ event_type: Event::Join, time, flag, powerups, team });
            }
            for _ in 0..num_returns {
                events.push(PlayerEvent{ event_type: Event::Return, time, flag, powerups, team });
            }
            for _ in 0..num_tags {
                events.push(PlayerEvent{ event_type: Event::Tag, time, flag, powerups, team });
            }
            if grab_occurred {
                flag = new_flag;
                events.push(PlayerEvent{ event_type: Event::Grab, time, flag, powerups, team });
            }
            while num_captures > 0 {
                num_captures -= 1;
                if flag_kept || flag == Flag::None {
                    events.push(PlayerEvent{ event_type: Event::FlaglessCapture, time, flag, powerups, team });
                } else {
                    events.push(PlayerEvent{ event_type: Event::Capture, time, flag, powerups, team });
                    flag = Flag::None;
                    flag_kept = true;
                }
            }

            let mut i: usize = 1;
            while i < 16 {
                if (powerups_lost & i) > 0 {
                    powerups ^= i;
                    events.push(PlayerEvent{ event_type: Event::Powerdown, time, flag, powerups, team });
                } else if (powerups_gained & i) > 0 {
                    powerups |= i;
                    events.push(PlayerEvent{ event_type: Event::Powerup, time, flag, powerups, team });
                }
                i <<= 1;
            }
            for _ in 0..num_new_powerups {
                events.push(PlayerEvent{ event_type: Event::DuplicatePowerup, time, flag, powerups, team });
            }

            if toggle_preventing {
                match preventing {
                    true => events.push(PlayerEvent{ event_type: Event::StopPrevent, time, flag, powerups, team }),
                    false => events.push(PlayerEvent{ event_type: Event::StartPrevent, time, flag, powerups, team })
                }
                preventing = !preventing;
            }
            if toggle_buttoning {
                match buttoning {
                    true => events.push(PlayerEvent{ event_type: Event::StopButton, time, flag, powerups, team }),
                    false => events.push(PlayerEvent{ event_type: Event::StartButton, time, flag, powerups, team })
                }
                buttoning = !buttoning;
            }
            if toggle_blocking {
                match blocking {
                    true => events.push(PlayerEvent{ event_type: Event::StopBlock, time, flag, powerups, team }),
                    false => events.push(PlayerEvent{ event_type: Event::StartBlock, time, flag, powerups, team })
                }
                blocking = !blocking;
            }

            if pop_occurred {
                if flag != Flag::None {
                    events.push(PlayerEvent{ event_type: Event::Drop, time, flag, powerups, team });
                    flag = Flag::None;
                } else {
                    events.push(PlayerEvent{ event_type: Event::Pop, time, flag, powerups, team });
                }
            }

            match new_team {
                x if x == team => (),
                Team::None => {
                    events.push(PlayerEvent{ event_type: Event::Quit, time, flag, powerups, team });
                    flag = Flag::None;
                    powerups = 0;
                },
                _ => {
                    events.push(PlayerEvent{ event_type: Event::Switch, time, flag, powerups, team });
                    flag = Flag::None;
                }
            }
        }
        events.push(PlayerEvent{ event_type: Event::End, time: duration, flag, powerups, team });
        events
    }

    pub fn map_layout(&mut self, width: usize) -> MapLayout {
        self.pos = 0;
        let mut layout: Vec<MapTile> = Vec::new();
        while self.events_remaining() || (layout.len() % width) == 0 {
            let tile: MapTile = MapTile::from_usize(match self.read_fixed(6) {
                0 => 0,
                n if n < 6 => n + 9,
                n if n < 13 => n * 10 - 40,
                n if n < 17 => n + 77,
                n if n < 20 => n * 10 - 70,
                n if n < 22 => n + 110,
                n => n * 10 - 80
            }).unwrap();

            for _ in 0..self.read_footer() + 1 {
                layout.push(tile);
            }
        }

        let height = layout.len() / width;
        MapLayout {
            layout,
            width,
            height
        }
    }

    fn bits_used_to_represent_coordinate(&self, num_tiles: usize) -> (usize, usize) {
        let highest_pixel_coordinate = 40 * num_tiles - 1;
        let mut bits_used: usize = 32;
        if highest_pixel_coordinate & 0xFFFF0000 == 0 {
            bits_used -= 16;
        }
        if highest_pixel_coordinate & 0x0000FF00 == 0 {
            bits_used -= 8;
        }
        if highest_pixel_coordinate & 0x000000F0 == 0 {
            bits_used -= 4;
        }
        if highest_pixel_coordinate & 0x0000000C == 0 {
            bits_used -= 2;
        }
        if highest_pixel_coordinate & 0x00000002 == 0 {
            bits_used -= 1;
        }

        let unused_space = ((1 << bits_used) - 40 * (num_tiles - 1)) / 2;
        (bits_used, unused_space)
    }

    pub fn splat_events(&mut self, map_layout: MapLayout) -> Vec<SplatEvent> {
        self.pos = 0;
        let x_bits = self.bits_used_to_represent_coordinate(map_layout.width);
        let y_bits = self.bits_used_to_represent_coordinate(map_layout.height);
        let mut splats: Vec<SplatEvent> = Vec::new();
        let mut time = 0;
        while self.events_remaining() {
            time += 1;
            for _ in 0..self.read_tally() {
                splats.push(SplatEvent {
                    x: self.read_fixed(x_bits.0) - x_bits.1,
                    y: self.read_fixed(y_bits.0) - y_bits.1,
                    time
                })
            }
        }
        splats
    }
}