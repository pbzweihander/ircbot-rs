# ircbot-rs

[![circleci](https://circleci.com/gh/pbzweihander/ircbot-rs.svg?style=shield)](https://circleci.com/gh/pbzweihander/ircbot-rs)
[![License](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Persional IRC bot written in Rust.

```
<@User> d resist
<bot> resist  [rizíst]  저항하다, 반대하다, 참다, 저지하다
<bot> resist  [rizíst]  elude, especially in a baffling way

<@User> pm 관악구
<bot> 측정소: 서울 동작구 사당로16아길 6 사당4동 주민센터
<bot> PM-10: 178㎍/㎥ 매우 나쁨
<bot> PM-2.5: 154㎍/㎥ 매우 나쁨

<@User> w answer to the ultimate question of life, the universe, and everything
<bot> Wolfram|Alpha 검색 중...
<bot> answer to the ultimate question of life, the universe, and everything ⇒ 42 https://i.imgur.com/qsYmol0.gif

<@User> h rust file io
<bot> Answer from: https://stackoverflow.com/questions/31192956/whats-the-de-facto-way-of-reading-and-writing-files-in-rust-1-x
<bot> use std::fs;
<bot> fn main() {
<bot>     let data = fs::read_to_string("/etc/hosts").expect("Unable to read file");
<bot>     println!("{}", data);
<bot> }
```

## Docker

```bash
docker run --rm -it -v $PWD/config.toml:/config.toml --name ircbot pbzweihander/ircbot:latest
```
