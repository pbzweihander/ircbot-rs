use {
    backslash_z::{Config as BzConfig, Request, Response},
    failure,
    futures::prelude::*,
    irc::client::prelude::*,
    lazy_static::lazy_static,
    regex::Regex,
    std::str::FromStr,
};

use std::env::args;
use std::path::PathBuf;

macro_rules! get_daummap_app_key {
    ($config:expr) => {
        $config
            .options
            .as_ref()
            .and_then(|hm| hm.get("daummap_app_key"))
    };
}

lazy_static! {
    static ref CONFIG_PATH: PathBuf =
        PathBuf::from(args().nth(1).unwrap_or_else(|| "config.toml".to_owned()));
    static ref CONFIG: Config = Config::load(&*CONFIG_PATH).unwrap();
    static ref BZ_CONFIG: BzConfig = BzConfig {
        daummap_app_key: get_daummap_app_key!(CONFIG)
            .expect("Expected a daummap app key in the config")
            .clone(),
    };
    static ref BRIDGE_USERNAME_REGEX: Regex = Regex::new("^<.*?> ").unwrap();
}

fn format_response(resp: Response) -> Vec<String> {
    use {airkorea::Grade, std::iter::once};

    match resp {
        Response::Dictionary(ref search) => {
            if !search.alternatives.is_empty() {
                vec![search.alternatives.join(", ")]
            } else {
                search
                    .words
                    .iter()
                    .map(|word| format!("{}", word))
                    .collect()
            }
        }
        Response::AirPollution(ref status) => {
            once(format!("{}, {}", status.station_address, status.time))
                .chain(status.pollutants.iter().map(|p| {
                    format!(
                        "{} ({}): {}  {}",
                        p.name,
                        p.unit,
                        p.data
                            .iter()
                            .skip(p.data.len() - 5)
                            .map(|p| p.map(|f| f.to_string()).unwrap_or_else(|| "--".to_string()))
                            .collect::<Vec<_>>()
                            .join(" → "),
                        match p.grade {
                            Grade::None => "정보없음",
                            Grade::Good => "좋음",
                            Grade::Normal => "보통",
                            Grade::Bad => "나쁨",
                            Grade::Critical => "매우나쁨",
                        },
                    )
                }))
                .collect()
        }
        Response::HowTo(answer) => {
            let answer: Vec<_> = once(format!("Answer from: {}", answer.link))
                .chain(
                    answer
                        .instruction
                        .split('\n')
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string),
                )
                .collect();

            if answer.len() > 9 {
                answer
                    .into_iter()
                    .take(5)
                    .chain(once("...(Check the link)".to_string()))
                    .collect()
            } else {
                answer
            }
        }
    }
}

fn send_privmsgs(
    client: IrcClient,
    channel: &str,
    msgs: Vec<String>,
) -> Result<(), failure::Error> {
    msgs.into_iter()
        .map(|m| client.send_privmsg(channel, &m).map_err(Into::into))
        .fold(Ok(()), |acc, res| if res.is_ok() { acc } else { res })
}

fn process_privmsg(
    client: IrcClient,
    channel: String,
    message: String,
) -> impl Future<Item = (), Error = failure::Error> {
    use futures::future::{result, ok, Either};

    let message = BRIDGE_USERNAME_REGEX.replace(&message, "");

    result(Request::from_str(&message)).then(|req| {
        if let Ok(req) = req {
            let fut = req.request(&BZ_CONFIG).then(|resp| {
                let r = match resp {
                    Ok(resp) => send_privmsgs(client, &channel, format_response(resp)),
                    Err(why) => {
                        eprintln!("Error while requesting: {:?}", why);
                        client.send_privmsg(channel, "._.").map_err(Into::into)
                    }
                };
                if let Err(why) = r {
                    eprintln!("Error while sending message: {:?}", why);
                }
                Ok(())
            });

            Either::A(fut)
        } else {
            Either::B(ok(()))
        }
    })
}

fn process_invite(client: &IrcClient, channel: &str, nickname: &str) -> Result<(), failure::Error> {
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

fn process_kick(client: &IrcClient, channel: &str, nickname: &str) -> Result<(), failure::Error> {
    if nickname == client.current_nickname() {
        Config::load(&*CONFIG_PATH)
            .and_then(|mut config| {
                config.channels.as_mut().unwrap().retain(|c| c != channel);
                config.save(&*CONFIG_PATH)
            })
            .map_err(Into::into)
    } else {
        Ok(())
    }
}

fn main() -> Result<(), failure::Error> {
    use futures::future::{ok, result, Either};

    openssl_probe::init_ssl_cert_env_vars();

    let mut reactor = IrcReactor::new()?;
    let client = reactor.prepare_client_and_connect(&CONFIG)?;
    client.identify()?;

    reactor.register_client_with_handler(client, move |client, msg| {
        let result = match msg.command {
            Command::PRIVMSG(channel, message) => {
                Either::A(process_privmsg(client.clone(), channel, message))
            }
            Command::INVITE(nickname, channel) => Either::B(Either::A(result(process_invite(
                &client, &channel, &nickname,
            )))),
            Command::KICK(channel, nickname, _) => Either::B(Either::B(Either::A(result(
                process_kick(&client, &channel, &nickname),
            )))),
            _ => Either::B(Either::B(Either::B(ok(())))),
        };

        result.then(|r| match r {
            Ok(_) => Ok(()),
            Err(why) => {
                eprintln!("Command error: {:?}", why);
                Ok(())
            }
        })
    });
    reactor.run()?;
    Ok(())
}
