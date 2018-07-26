FROM bitnami/minideb:stretch

RUN install_packages libssl-dev ca-certificates

COPY target/release/ircbot /ircbot
COPY config.toml /config.toml

CMD [ "/ircbot", "/config.toml" ]
