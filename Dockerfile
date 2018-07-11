FROM rust:latest

WORKDIR /usr/src/ircbot

COPY . .
COPY ./config.toml /etc/ircbot/config.toml

RUN cargo install --path .

CMD [ "ircbot", "/etc/ircbot/config.toml" ]
