# tokio-framed-slow

See https://github.com/tokio-rs/tokio/issues/3417

```
$ cargo run --release
   Compiling bench v0.1.0 (/home/aidanhs/work/tokio-framed-slow)
    Finished release [optimized] target(s) in 1.14s
     Running `target/release/bench`
Using a single threaded tokio executor
Sending data over an in-memory channel with tokio_serde::Framed
sending 1983MB
t1 14.313900855s
Sending data over an in-memory channel with tokio_serde::Framed and a BufReader on the recv side
sending 1983MB
t2 8.703315058s
```
