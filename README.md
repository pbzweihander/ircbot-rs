# ircbot-rs

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

Persional IRC bot written in Rust.

```console
<@User> d resist
<bot> resist  [rizíst]  저항하다, 반대하다, 참다, 저지하다
<@User> pm 관악구
<bot> 측정소: 서울 동작구 사당로16아길 6 사당4동 주민센터
<bot> PM-10 (㎍/㎥): 68 → 68 → 77 → 90 → 90 → 139 → 178 (매우 나쁨)
<bot> PM-2.5 (㎍/㎥): 37 → 39 → 48 → 60 → 62 → 116 → 154 (매우 나쁨)
<@User> w integral sqrt tan x
<bot> Wolfram|Alpha 검색 중...
<bot> integral sqrt tan x ⇒ (-2 tan^(-1)(1 - sqrt(2) sqrt(tan(x))) + 2 tan^(-1)(sqrt(2) sqrt(tan(x)) + 1) + log(tan(x) - sqrt(2) sqrt(tan(x)) + 1) - log(tan(x) + sqrt(2) sqrt(tan(x)) + 1))/(2 sqrt(2)) https://i.imgur.com/l7jTcPu.gif
```

## Docker

```bash
docker run --rm -it -v $PWD/config.toml:/config.toml --name ircbot pbzweihander/ircbot:latest
```
