FROM alpine:latest
RUN apk --no-cache add ca-certificates

COPY ircbot /ircbot
COPY config.default.toml /config.toml

CMD [ "/ircbot", "/config.toml" ]
