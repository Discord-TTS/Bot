FROM rustlang/rust:nightly as builder

WORKDIR /bot

RUN apt-get update && apt-get install -y cmake mold && apt-get clean

COPY . .
RUN cargo build --release

# Now make the runtime container
FROM debian:trixie-slim

RUN apt-get update && apt-get upgrade -y && apt-get install -y ca-certificates mold && rm -rf /var/lib/apt/lists/*

COPY --from=builder /bot/target/release/tts_bot /usr/local/bin/discord_tts_bot

CMD ["/usr/local/bin/discord_tts_bot"]
