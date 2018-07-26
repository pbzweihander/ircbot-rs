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
extern crate failure;
extern crate tokio_core;

use failure::Error;
use futures::future::ok;
use futures::prelude::*;
use futures::stream;
use futures::sync::mpsc::{channel, Sender};
use irc::client::prelude::*;
use regex::Regex;
use reqwest::unstable::async as request;
use tokio_core::reactor::Handle;

use std::env::args;
use std::path::PathBuf;

type Result<T> = std::result::Result<T, failure::Error>;

lazy_static! {
    static ref CONFIG_PATH: PathBuf =
        PathBuf::from(args().nth(1).unwrap_or_else(|| "config.toml".to_owned()));
    static ref CONFIG: Config = Config::load(&*CONFIG_PATH).unwrap();
}

macro_rules! get_daummap_app_key {
    ($config:expr) => {
        $config.get_option("daummap_app_key")
    };
}

macro_rules! get_wolfram_app_id {
    ($config:expr) => {
        $config.get_option("wolfram_app_id")
    };
}

macro_rules! get_imgur_client_id {
    ($config:expr) => {
        $config.get_option("imgur_client_id")
    };
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

fn search_dic(query: &str) -> Option<Vec<String>> {
    use std::iter::once;

    daumdic::search(query).ok().map(|res| {
        let words = res.words;
        let alternatives = res.alternatives;

        let v: Vec<_> = once(alternatives.join(", "))
            .chain(words.into_iter().map(|w| format!("{}", w)))
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
        .or_else(|| Some(vec![]))
}

fn search_wolfram(
    channel: &str,
    query: &str,
    wolfram_tx: Sender<(String, String)>,
) -> Option<Vec<String>> {
    wolfram_tx
        .send((channel.to_owned(), query.to_owned()))
        .map(|_| vec!["Wolfram|Alpha 검색 중...".to_owned()])
        .wait()
        .ok()
}

fn search_wolfram_real(
    handle: &Handle,
    query: &str,
    wolfram_app_id: &str,
    imgur_client_id: &str,
) -> impl Future<Item = Vec<String>, Error = Error> {
    #[derive(Deserialize)]
    struct ImgurResponse {
        data: ImgurResponseData,
    }
    #[derive(Deserialize)]
    struct ImgurResponseData {
        link: String,
    }

    let query_for_url = query.replace("+", "%2B");
    let q1 = query.to_owned();
    let q2 = query.to_owned();
    let imgur_client_id = imgur_client_id.to_owned();
    let h1 = handle.clone();
    let h2 = handle.clone();
    let h3 = handle.clone();

    format!(
        "http://api.wolframalpha.com/v1/result?appid={}&i={}&units=metric",
        wolfram_app_id, query_for_url
    ).parse::<reqwest::Url>()
        .map_err::<Error, _>(|e| e.into())
        .into_future()
        .and_then(move |uri| {
            request::Client::new(&h1)
                .get(uri)
                .send()
                .map_err(|e| e.into())
        })
        .and_then(|resp| {
            resp.into_body()
                .map_err::<Error, _>(|e| e.into())
                .map(|chunk| stream::iter_ok::<_, Error>(chunk.into_iter()))
                .flatten()
                .take(300)
                .collect()
        })
        .and_then(|v| String::from_utf8(v).map_err(|e| e.into()).into_future())
        .join(
            format!(
                "http://api.wolframalpha.com/v1/simple?appid={}&i={}&units=metric",
                wolfram_app_id, query_for_url
            ).parse::<reqwest::Url>()
                .map_err::<Error, _>(|e| e.into())
                .into_future()
                .and_then(move |uri| {
                    request::Client::new(&h2)
                        .get(uri)
                        .send()
                        .map_err(|e| e.into())
                })
                .and_then(|resp| {
                    resp.into_body()
                        .map_err::<Error, _>(|e| e.into())
                        .map(|chunk| stream::iter_ok::<_, Error>(chunk.into_iter()))
                        .flatten()
                        .collect()
                })
                .map(|img| base64::encode(&img))
                .and_then(move |img| {
                    request::Client::new(&h3)
                        .post("https://api.imgur.com/3/image")
                        .header(reqwest::header::Authorization(format!(
                            "Client-ID {}",
                            imgur_client_id
                        )))
                        .multipart(
                            reqwest::multipart::Form::new()
                                .text("image", img)
                                .text("title", q1),
                        )
                        .send()
                        .map_err(|e| e.into())
                })
                .and_then(|mut resp| resp.json::<ImgurResponse>().map_err(|e| e.into()))
                .map(|resp| resp.data.link),
        )
        .map(move |(simple, url)| vec![q2 + " ⇒ " + &simple + " " + &url])
        .or_else(|_| ok(vec![]))
}

enum BotCommand {
    Dictionary(String),
    AirPollution(String, String),
    WolframAlpha(String),
}

impl BotCommand {
    fn from_str(message: &str) -> Option<Self> {
        lazy_static! {
            static ref REGEX_DIC: Regex = Regex::new(r"^[dD](?:ic)? (.+)$").unwrap();
            static ref REGEX_AIR: Regex =
                Regex::new(r"^(air|pm|pm10|pm25|o3|so2|no2|co|so2) (.+)$").unwrap();
            static ref REGEX_WOLFRAM: Regex = Regex::new(r"^[wW](?:olfram)? (.+)$").unwrap();
        }

        REGEX_DIC
            .captures(message)
            .map(|c| c.get(1).unwrap().as_str().to_owned())
            .map(|s| BotCommand::Dictionary(s))
            .or_else(|| {
                REGEX_AIR
                    .captures(message)
                    .map(|c| {
                        (
                            c.get(1).unwrap().as_str().to_owned(),
                            c.get(2).unwrap().as_str().to_owned(),
                        )
                    })
                    .map(|(s1, s2)| BotCommand::AirPollution(s1, s2))
            })
            .or_else(|| {
                REGEX_WOLFRAM
                    .captures(message)
                    .map(|c| c.get(1).unwrap().as_str().to_owned())
                    .map(|s| BotCommand::WolframAlpha(s))
            })
    }

    fn process(self, channel: &str, wolfram_tx: Sender<(String, String)>) -> Option<Vec<String>> {
        match self {
            BotCommand::Dictionary(query) => search_dic(&query),
            BotCommand::AirPollution(command, query) => {
                search_air(&command, &query, get_daummap_app_key!(CONFIG))
            }
            BotCommand::WolframAlpha(query) => search_wolfram(channel, &query, wolfram_tx),
        }
    }
}

fn send_privmsgs(client: &IrcClient, channel: &str, msgs: Vec<String>) -> Result<()> {
    msgs.into_iter()
        .map(|m| {
            client
                .send_privmsg(channel, &m)
                .map_err::<Error, _>(Into::into)
        })
        .fold(Ok(()), |acc, res| if res.is_ok() { acc } else { res })
}

fn wolfram_future(
    handle: &Handle,
    client: &IrcClient,
    rx: futures::sync::mpsc::Receiver<(String, String)>,
) -> impl Future<Item = (), Error = ()> + 'static {
    let handle = handle.clone();
    let client = client.clone();
    rx.for_each(move |(channel, query)| {
        let client = client.clone();
        search_wolfram_real(
            &handle,
            &query,
            get_wolfram_app_id!(CONFIG),
            get_imgur_client_id!(CONFIG),
        ).and_then(move |msgs| send_privmsgs(&client, &channel, msgs).map_err(Into::into))
            .map_err(|e| {
                eprintln!("{}", e);
                ()
            })
    })
}

fn process_privmsg(
    client: &IrcClient,
    channel: &str,
    message: &str,
    tx: futures::sync::mpsc::Sender<(String, String)>,
) -> Result<()> {
    let msgs = BotCommand::from_str(message)
        .and_then(|c| c.process(channel, tx))
        .map(|v| {
            if v.is_empty() {
                vec!["._.".to_owned()]
            } else {
                v
            }
        })
        .unwrap_or_default();
    send_privmsgs(client, &channel, msgs)
}

fn process_invite(client: &IrcClient, channel: &str, nickname: &str) -> Result<()> {
    if nickname == client.current_nickname() {
        client
            .send_join(&channel)
            .and_then(|_| {
                let mut config = Config::load(&*CONFIG_PATH).unwrap();
                config.channels.as_mut().unwrap().push(channel.to_owned());
                config.save(&*CONFIG_PATH)
            })
            .map_err(Into::into)
    } else {
        Ok(())
    }
}

fn process_kick(client: &IrcClient, channel: &str, nickname: &str) -> Result<()> {
    if nickname == client.current_nickname() {
        Config::load(&*CONFIG_PATH)
            .and_then(|mut config| {
                config.channels.as_mut().unwrap().retain(|c| c != &channel);
                config.save(&*CONFIG_PATH)
            })
            .map_err(Into::into)
    } else {
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut reactor = IrcReactor::new()?;
    let client = reactor.prepare_client_and_connect(&CONFIG)?;
    let cloned_client = client.clone();
    client.identify()?;
    let handle = reactor.inner_handle();
    let (tx, rx) = channel::<(String, String)>(5);

    reactor
        .register_future(wolfram_future(&handle, &cloned_client, rx).map_err(|_| {
            irc::error::IrcError::Io(std::io::Error::from(std::io::ErrorKind::Other))
        }));

    reactor.register_client_with_handler(client, move |client, msg| {
        let tx = tx.clone();
        let result = match msg.command {
            Command::PRIVMSG(channel, message) => process_privmsg(&client, &channel, &message, tx),
            Command::INVITE(nickname, channel) => process_invite(&client, &channel, &nickname),
            Command::KICK(channel, nickname, _) => process_kick(&client, &channel, &nickname),
            _ => Ok(()),
        };

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("{}", e.to_string());
                Ok(())
            }
        }
    });
    reactor.run()?;
    Ok(())
}
