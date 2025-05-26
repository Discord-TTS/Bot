FROM rustlang/rust:nightly as builder
ENV RUSTFLAGS="-C target-cpu=skylake"

WORKDIR /bot

RUN apt-get update && apt-get install -y cmake && apt-get clean

COPY . .
RUN cargo build --release

# Now make the runtime container
FROM debian:bookworm-slim

RUN apt-get update && apt-get upgrade -y && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /bot/target/release/tts_bot /usr/local/bin/discord_tts_bot

CMD ["/usr/local/bin/discord_tts_bot"]
