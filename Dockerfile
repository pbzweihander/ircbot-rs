FROM rust:1
WORKDIR /usr/src/ircbot
COPY . .
RUN cargo build --release

FROM bitnami/minideb:stretch
COPY --from=0 /usr/src/ircbot/target/release/ircbot /ircbot
COPY ./config.toml /config.toml

CMD [ "/ircbot", "/config.toml" ]
