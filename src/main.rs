extern crate airkorea;
extern crate base64;
extern crate daumdic;
extern crate daummap;
extern crate futures;
extern crate irc;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;

use futures::prelude::*;
use futures::stream;
use futures::future::ok;
use irc::client::prelude::*;
use irc::error::IrcError;
use regex::Regex;
use reqwest::unstable::async as request;
use tokio_core::reactor::Handle;

use std::env::args;
use std::path::PathBuf;

lazy_static! {
    static ref CONFIG_PATH: PathBuf =
        PathBuf::from(args().nth(1).unwrap_or_else(|| "config.toml".to_owned()));
    static ref CONFIG: Config = Config::load(&*CONFIG_PATH).unwrap();
}

macro_rules! get_daummap_app_key {
    ($config:expr) => {
        $config.get_option("daummap_app_key").to_owned()
    }
}

macro_rules! get_wolfram_app_id {
    ($config:expr) => {
        $config.get_option("wolfram_app_id").to_owned()
    }
}

macro_rules! get_imgur_client_id {
    ($config:expr) => {
        $config.get_option("imgur_client_id").to_owned()
    }
}

fn main() {
    let mut reactor = IrcReactor::new().unwrap();

    loop {
        match reactor
            .prepare_client_and_connect(&CONFIG)
            .and_then(|client| {
                client.identify().and_then(|_| {
                    let handle = reactor.inner_handle();
                    reactor.register_future(client.stream().for_each(move |msg| {
                        let client = client.clone();
                        let handle = handle.clone();

                        match msg.command {
                            Command::PRIVMSG(channel, message) => {
                                handle_privmsg(handle, client, channel, message)
                            }
                            Command::INVITE(nickname, channel) => {
                                handle_invite(&client, &nickname, &channel)
                            }
                            Command::KICK(channel, nickname, _) => {
                                handle_kick(&client, &nickname, &channel)
                            }
                            _ => do_nothing(),
                        }
                    }));
                    reactor.run()
                })
            }) {
            Ok(_) => break,
            Err(e) => eprintln!("{}", e),
        }
    }
}

fn handle_privmsg(
    handle: Handle,
    client: IrcClient,
    channel: String,
    message: String,
) -> Box<Future<Item = (), Error = IrcError>> {
    Box::new(
        process_message(handle, message)
            .map(|v| {
                if v.is_empty() {
                    vec!["._.".to_owned()]
                } else {
                    v
                }
            })
            .or_else(|_| ok(vec![]))
            .and_then(move |msgs| {
                msgs.into_iter()
                    .map(|msg| client.send_privmsg(&channel, &msg))
                    .fold(Ok(()), |acc, r| if r.is_ok() { acc } else { r })
                    .into_future()
            }),
    )
}

fn handle_invite(
    client: &IrcClient,
    nickname: &str,
    channel: &str,
) -> Box<Future<Item = (), Error = IrcError>> {
    if nickname == client.current_nickname() {
        client.send_join(channel).unwrap();
        let mut config = Config::load(&*CONFIG_PATH).unwrap();
        config.channels.as_mut().unwrap().push(channel.to_owned());
        config.save(&*CONFIG_PATH).unwrap();
    }
    Box::new(ok(()))
}

fn handle_kick(
    client: &IrcClient,
    nickname: &str,
    channel: &str,
) -> Box<Future<Item = (), Error = IrcError>> {
    if nickname == client.current_nickname() {
        let mut config = Config::load(&*CONFIG_PATH).unwrap();
        config.channels.as_mut().unwrap().retain(|c| c != channel);
        config.save(&*CONFIG_PATH).unwrap();
    }
    Box::new(ok(()))
}

fn do_nothing() -> Box<Future<Item = (), Error = IrcError>> {
    Box::new(ok(()))
}

fn process_message(handle: Handle, message: String) -> Box<Future<Item = Vec<String>, Error = ()>> {
    let m = message.clone();
    Box::new(
        parse_dic(&message)
            .and_then(|word| search_dic(&word))
            .or_else(move |_| {
                parse_air(&message.clone()).and_then(|(command, query)| {
                    search_air(command, &query, &get_daummap_app_key!(CONFIG))
                })
            })
            .or_else(move |_| {
                Box::new(parse_wolfram(&m).and_then(|query| {
                    search_wolfram(
                        handle,
                        &query,
                        &get_wolfram_app_id!(CONFIG),
                        get_imgur_client_id!(CONFIG),
                    )
                }))
            }),
    )
}

fn parse_dic(message: &str) -> Box<Future<Item = String, Error = ()>> {
    lazy_static! {
        static ref REGEX_DIC: Regex =
            Regex::new(r"^[dD](?:ic)? (.+)$").unwrap();
    }
    Box::new(
        REGEX_DIC
            .captures(message)
            .map(|c| c.get(1).unwrap().as_str().to_owned())
            .ok_or_else(|| ())
            .into_future(),
    )
}

fn parse_air(message: &str) -> Box<Future<Item = (String, String), Error = ()>> {
    lazy_static! {
        static ref REGEX_AIR: Regex =
            Regex::new(r"^(air|pm|pm10|pm25|o3|so2|no2|co|so2) (.+)$").unwrap();
    }
    Box::new(
        REGEX_AIR
            .captures(message)
            .map(|c| {
                (
                    c.get(1).unwrap().as_str().to_owned(),
                    c.get(2).unwrap().as_str().to_owned(),
                )
            })
            .ok_or_else(|| ())
            .into_future(),
    )
}

fn parse_wolfram(message: &str) -> Box<Future<Item = String, Error = ()>> {
    lazy_static! {
        static ref REGEX_WOLFRAM: Regex =
            Regex::new(r"^[wW](?:olfram)? (.+)$").unwrap();
    }
    Box::new(
        REGEX_WOLFRAM
            .captures(message)
            .map(|c| c.get(1).unwrap().as_str().to_owned())
            .ok_or_else(|| ())
            .into_future(),
    )
}

fn search_dic(query: &str) -> Box<Future<Item = Vec<String>, Error = ()>> {
    use std::iter::once;
    Box::new(
        daumdic::search(query)
            .ok()
            .map(|res| {
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
            .ok_or_else(|| ())
            .into_future(),
    )
}

fn search_air(
    command: String,
    query: &str,
    app_key: &str,
) -> Box<Future<Item = Vec<String>, Error = ()>> {
    use std::iter::once;

    Box::new(
        ok(daummap::AddressRequest::new(app_key, query)
            .get()
            .filter_map(|address| get_coord_from_address(&address))
            .next())
            .join(ok(daummap::KeywordRequest::new(app_key, query)
            .get()
            .filter_map(|place| get_coord_from_place(&place))
            .next()))
            .and_then(|(address, keyword)| {
                if address.is_some() {
                    address.ok_or_else(|| ()).into_future()
                } else {
                    keyword.ok_or_else(|| ()).into_future()
                }
            })
            .and_then(|(longitude, latitude)| {
                airkorea::search(longitude, latitude)
                    .map_err(|_| ())
                    .into_future()
            })
            .and_then(move |status| {
                let station_address = status.station_address.clone();
                match command.as_ref() {
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
                    .ok_or_else(|| ())
            })
            .or_else(|_| ok(vec![])),
    )
}

fn search_wolfram(
    handle: Handle,
    query: &str,
    wolfram_app_id: &str,
    imgur_client_id: String,
) -> Box<Future<Item = Vec<String>, Error = ()>> {
    #[derive(Deserialize)]
    struct ImgurResponse {
        data: ImgurResponseData,
    }
    #[derive(Deserialize)]
    struct ImgurResponseData {
        link: String,
    }

    let h1 = handle.clone();
    let h2 = handle.clone();
    let query = query.replace("+", "%2B");

    Box::new(
        format!(
            "http://api.wolframalpha.com/v1/result?appid={}&i={}&units=metric",
            wolfram_app_id, query
        ).parse::<reqwest::Url>()
            .map_err(|_| ())
            .into_future()
            .and_then(move |uri| {
                request::Client::new(&handle)
                    .get(uri)
                    .send()
                    .map_err(|_| ())
            })
            .and_then(|resp| {
                resp.into_body()
                    .map_err(|_| ())
                    .map(|chunk| stream::iter_ok::<_, ()>(chunk.into_iter()))
                    .flatten()
                    .collect()
            })
            .and_then(|v| String::from_utf8(v).map_err(|_| ()).into_future())
            .join(
                format!(
                    "http://api.wolframalpha.com/v1/simple?appid={}&i={}&units=metric",
                    wolfram_app_id, query
                ).parse::<reqwest::Url>()
                    .map_err(|_| ())
                    .into_future()
                    .and_then(move |uri| request::Client::new(&h1).get(uri).send().map_err(|_| ()))
                    .and_then(|resp| {
                        resp.into_body()
                            .map_err(|_| ())
                            .map(|chunk| stream::iter_ok::<_, ()>(chunk.into_iter()))
                            .flatten()
                            .collect()
                    })
                    .map(|img| base64::encode(&img))
                    .and_then(move |img| {
                        request::Client::new(&h2)
                            .post("https://api.imgur.com/3/image")
                            .header(reqwest::header::Authorization(format!(
                                "Client-ID {}",
                                imgur_client_id
                            )))
                            .multipart(
                                reqwest::multipart::Form::new()
                                    .text("image", img)
                                    .text("title", query.to_owned()),
                            )
                            .send()
                            .map_err(|_| ())
                    })
                    .and_then(|mut resp| resp.json::<ImgurResponse>().map_err(|_| ()))
                    .map(|resp| resp.data.link),
            )
            .map(|(simple, url)| vec![simple + " " + &url])
            .or_else(|_| ok(vec![])),
    )
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
