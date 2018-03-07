extern crate airkorea;
extern crate daumdic;
extern crate daummap;
extern crate irc;
#[macro_use]
extern crate lazy_static;
extern crate regex;

use irc::client::prelude::*;
use std::env::args;
use regex::Regex;
use std::path::PathBuf;

fn main() {
    let config_path = PathBuf::from(args().nth(1).unwrap_or_else(|| "config.toml".to_owned()));
    let config = Config::load(&config_path).unwrap();
    let app_key = config.get_option("daummap_app_key").to_owned();

    let mut reactor = IrcReactor::new().unwrap();
    let client = reactor.prepare_client_and_connect(&config).unwrap();
    client.identify().unwrap();

    reactor.register_client_with_handler(client, move |ref client, msg| {
        match msg.command {
            Command::PRIVMSG(channel, message) => {
                let msgs = parse_dic(&message)
                    .and_then(|word| search_dic(&word))
                    .or_else(|| {
                        parse_air(&message)
                            .and_then(|(command, query)| search_air(&command, &query, &app_key))
                    })
                    .unwrap_or_else(|| vec!["._.".to_owned()]);
                for msg in msgs {
                    client.send_privmsg(&channel, &msg).unwrap();
                }
            }
            Command::INVITE(nickname, channel) => {
                if nickname == client.current_nickname() {
                    client.send_join(&channel).unwrap();
                    let mut config = Config::load(&config_path).unwrap();
                    config.channels.as_mut().unwrap().push(channel);
                    config.save(&config_path).unwrap();
                }
            }
            Command::KICK(channel, nickname, _) => {
                if nickname == client.current_nickname() {
                    let mut config = Config::load(&config_path).unwrap();
                    config.channels.as_mut().unwrap().retain(|c| c != &channel);
                    config.save(&config_path).unwrap();
                }
            }
            _ => (),
        };
        Ok(())
    });
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
