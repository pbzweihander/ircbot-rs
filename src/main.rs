extern crate airkorea;
extern crate daumdic;
extern crate daummap;
extern crate irc;
#[macro_use]
extern crate lazy_static;
extern crate regex;

use irc::client::prelude::*;
use regex::Regex;

use std::env::args;
use std::path::PathBuf;

lazy_static! {
    static ref CONFIG_PATH: PathBuf =
        PathBuf::from(args().nth(1).unwrap_or_else(|| "config.toml".to_owned()));
    static ref CONFIG: Config = Config::load(&*CONFIG_PATH).unwrap();
}

macro_rules! get_daummap_app_key {
    ($config:expr) => {
        $config.get_option("daummap_app_key")
    }
}

fn main() {
    let mut reactor = IrcReactor::new().unwrap();
    loop {
        match reactor
            .prepare_client_and_connect(&CONFIG)
            .and_then(|client| {
                client.identify().and_then(|_| {
                    reactor.register_client_with_handler(client, move |client, msg| {
                        match msg.command {
                            Command::PRIVMSG(channel, message) => {
                                let msgs = process_message(&message)
                                    .map(|v| {
                                        if v.is_empty() {
                                            vec!["._.".to_owned()]
                                        } else {
                                            v
                                        }
                                    })
                                    .unwrap_or_default();
                                for msg in msgs {
                                    client.send_privmsg(&channel, &msg).unwrap();
                                }
                            }
                            Command::INVITE(nickname, channel) => {
                                if nickname == client.current_nickname() {
                                    client.send_join(&channel).unwrap();
                                    let mut config = Config::load(&*CONFIG_PATH).unwrap();
                                    config.channels.as_mut().unwrap().push(channel);
                                    config.save(&*CONFIG_PATH).unwrap();
                                }
                            }
                            Command::KICK(channel, nickname, _) => {
                                if nickname == client.current_nickname() {
                                    let mut config = Config::load(&*CONFIG_PATH).unwrap();
                                    config.channels.as_mut().unwrap().retain(|c| c != &channel);
                                    config.save(&*CONFIG_PATH).unwrap();
                                }
                            }
                            _ => (),
                        };
                        Ok(())
                    });
                    reactor.run()
                })
            }) {
            Ok(_) => break,
            Err(e) => eprintln!("{}", e),
        }
    }
}

fn process_message(message: &str) -> Option<Vec<String>> {
    parse_dic(message)
        .and_then(|word| search_dic(&word))
        .or_else(|| {
            parse_air(message).and_then(|(command, query)| {
                search_air(&command, &query, get_daummap_app_key!(CONFIG))
            })
        })
}

fn parse_dic(message: &str) -> Option<String> {
    lazy_static! {
        static ref REGEX_DIC: Regex =
            Regex::new(r"^[dD](?:ic)? (.+)$").unwrap();
    }
    REGEX_DIC
        .captures(message)
        .map(|c| c.get(1).unwrap().as_str().to_owned())
}

fn parse_air(message: &str) -> Option<(String, String)> {
    lazy_static! {
        static ref REGEX_AIR: Regex =
            Regex::new(r"^(air|pm|pm10|pm25|o3|so2|no2|co|so2) (.+)$").unwrap();
    }
    REGEX_AIR.captures(message).map(|c| {
        (
            c.get(1).unwrap().as_str().to_owned(),
            c.get(2).unwrap().as_str().to_owned(),
        )
    })
}

fn search_dic(query: &str) -> Option<Vec<String>> {
    use std::iter::once;

    daumdic::search(query).ok().map(|res| {
        let word = res.word;
        let alternatives = res.alternatives;

        let v: Vec<_> = once(alternatives.join(", "))
            .chain(once(
                word.map(|word| format!("{}", word)).unwrap_or_default(),
            ))
            .filter(|s| !s.is_empty())
            .collect();
        v
    })
}

fn search_air(command: &str, query: &str, app_key: &str) -> Option<Vec<String>> {
    use std::iter::once;

    daummap::AddressRequest::new(app_key, query)
        .get()
        .filter_map(|address| get_coord_from_address(&address))
        .next()
        .or_else(|| {
            daummap::KeywordRequest::new(app_key, query)
                .get()
                .filter_map(|place| get_coord_from_place(&place))
                .next()
        })
        .and_then(|(longitude, latitude)| airkorea::search(longitude, latitude).ok())
        .and_then(|status| {
            let station_address = status.station_address.clone();
            match command {
                "air" => Some(
                    status
                        .into_iter()
                        .map(|pollutant| format_pollutant_with_name(&pollutant))
                        .collect::<Vec<_>>(),
                ),
                "pm" => Some(
                    status
                        .into_iter()
                        .take(2)
                        .map(|pollutant| format_pollutant_with_name(&pollutant))
                        .collect::<Vec<_>>(),
                ),
                command => status
                    .into_map()
                    .get(command)
                    .map(format_pollutant_with_name)
                    .map(|res| vec![res]),
            }.map(|res| {
                once(format!("측정소: {}", station_address))
                    .chain(res)
                    .collect::<Vec<_>>()
            })
        })
}

fn join<T, U>(e: (Option<T>, Option<U>)) -> Option<(T, U)> {
    match e {
        (Some(t), Some(u)) => Some((t, u)),
        _ => None,
    }
}

fn get_coord_from_address(address: &daummap::Address) -> Option<(f32, f32)> {
    address
        .land_lot
        .as_ref()
        .map(|land_lot| (land_lot.longitude, land_lot.latitude))
        .and_then(join)
}

fn get_coord_from_place(place: &daummap::Place) -> Option<(f32, f32)> {
    join((place.longitude, place.latitude))
}

fn format_pollutant_with_name(pollutant: &airkorea::Pollutant) -> String {
    format!(
        "{} ({}): {} ({})",
        pollutant.name,
        pollutant.unit,
        (&pollutant.level_by_time)
            .into_iter()
            .map(|l| match *l {
                Some(f) => f.to_string(),
                None => String::new(),
            })
            .collect::<Vec<String>>()
            .join(" → "),
        match pollutant.grade {
            airkorea::Grade::None => "정보 없음",
            airkorea::Grade::Good => "좋음",
            airkorea::Grade::Normal => "보통",
            airkorea::Grade::Bad => "나쁨",
            airkorea::Grade::Critical => "매우 나쁨",
        }
    )
}
