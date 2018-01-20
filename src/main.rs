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

fn main() {
    let server =
        IrcServer::new(&args().nth(1).unwrap_or_else(|| "config.toml".to_owned())).unwrap();
    server.identify().unwrap();
    let app_key = server
        .config()
        .options
        .as_ref()
        .and_then(|m| m.get("daummap_app_key"))
        .unwrap();
    server
        .for_each_incoming(|msg| match msg.command {
            Command::PRIVMSG(channel, message) => {
                let msgs = dic(&message)
                    .or_else(|| air(&message, app_key))
                    .unwrap_or_else(|| vec![]);
                for msg in msgs {
                    server.send_privmsg(&channel, &msg).unwrap();
                }
            }
            Command::INVITE(nickname, channel) => {
                println!("{}, {}", nickname, channel);
                if nickname == server.current_nickname() {
                    server.send_join(&channel).unwrap();
                }
            }
            _ => (),
        })
        .unwrap();
}

fn dic(message: &str) -> Option<Vec<String>> {
    lazy_static! {
        static ref REGEX_DIC: Regex =
            Regex::new(r"^[dD](?:ic)? (.+)$").unwrap();
    }
    REGEX_DIC.captures(&message).map(|c| {
        let msg = match daumdic::search(c.get(1).unwrap().as_str()) {
            Ok(result) => format!("{}", result),
            Err(e) => match e {
                daumdic::Error(daumdic::ErrorKind::RelativeResultFound(words), _) => {
                    words.join(", ")
                }
                _ => "._.".to_owned(),
            },
        };
        vec![msg]
    })
}

fn air(message: &str, app_key: &str) -> Option<Vec<String>> {
    lazy_static! {
        static ref REGEX_AIR: Regex =
            Regex::new(r"^(air|pm|pm10|pm25|o3|so2|no2|co|so2) (.+)$").unwrap();
    }
    REGEX_AIR.captures(&message).map(|c| {
        daummap::AddressRequest::new(app_key, c.get(2).unwrap().as_str())
            .get()
            .filter_map(get_coord_from_address)
            .next()
            .or_else(|| {
                daummap::KeywordRequest::new(app_key, c.get(2).unwrap().as_str())
                    .get()
                    .filter_map(get_coord_from_place)
                    .next()
            })
            .and_then(|(longitude, latitude)| airkorea::search(longitude, latitude).ok())
            .map(|status| {
                let id = c.get(1).unwrap().as_str();
                let mut v: Vec<_> = vec![format!("측정소: {}", status.station_address.clone())];
                v.extend(match id {
                    "air" => status
                        .into_iter()
                        .map(|pollutant| format_pollutant_with_name(&pollutant))
                        .collect::<Vec<String>>(),
                    "pm" => status
                        .into_iter()
                        .take(2)
                        .map(|pollutant| format_pollutant_with_name(&pollutant))
                        .collect::<Vec<String>>(),
                    id => vec![
                        status
                            .into_map()
                            .get(id)
                            .map(format_pollutant_with_name)
                            .unwrap_or_else(|| "._.".to_owned()),
                    ],
                });
                v
            })
            .unwrap_or_else(|| vec!["._.".to_owned()])
    })
}

fn join<T, U>(e: (Option<T>, Option<U>)) -> Option<(T, U)> {
    match e {
        (Some(t), Some(u)) => Some((t, u)),
        _ => None,
    }
}

fn get_coord_from_address(address: daummap::Address) -> Option<(f32, f32)> {
    address
        .land_lot
        .map(|land_lot| (land_lot.longitude, land_lot.latitude))
        .and_then(join)
}

fn get_coord_from_place(place: daummap::Place) -> Option<(f32, f32)> {
    join((place.longitude, place.latitude))
}

fn format_pollutant_with_name<'a>(pollutant: &'a airkorea::Pollutant) -> String {
    format!(
        "{} ({}): {} {}",
        pollutant.name,
        pollutant.unit,
        (&pollutant.level_by_time)
            .into_iter()
            .map(|l| match *l {
                Some(f) => f.to_string(),
                None => String::new(),
            })
            .collect::<Vec<String>>()
            .join(" -> "),
        match pollutant.grade {
            airkorea::Grade::None => "",
            airkorea::Grade::Good => "(좋음)",
            airkorea::Grade::Normal => "(보통)",
            airkorea::Grade::Bad => "(나쁨)",
            airkorea::Grade::Critical => "(매우 나쁨)",
        }
    )
}
