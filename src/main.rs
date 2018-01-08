extern crate daumdic;
extern crate irc;
#[macro_use]
extern crate lazy_static;
extern crate regex;

use irc::client::prelude::*;

fn main() {
    let server = IrcServer::new("config.toml").unwrap();
    server.identify().unwrap();
    server
        .for_each_incoming(|msg| match msg.command {
            Command::PRIVMSG(channel, message) => {
                lazy_static! {
                    static ref RE: regex::Regex = regex::Regex::new(r"^[dD](?:ic)? (.+)$").unwrap();
                }
                if let Some(c) = RE.captures(&message) {
                    server
                        .send_privmsg(
                            &channel,
                            &match daumdic::search(c.get(1).unwrap().as_str()) {
                                Ok(result) => format!("{}", result),
                                Err(e) => match e {
                                    daumdic::Error(
                                        daumdic::ErrorKind::RelativeResultFound(words),
                                        _,
                                    ) => words.join(", "),
                                    _ => "._.".to_owned(),
                                },
                            },
                        )
                        .unwrap();
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
