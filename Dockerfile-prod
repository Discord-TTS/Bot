FROM rustlang/rust:nightly as builder
ENV RUSTFLAGS="-C target-cpu=native"

WORKDIR /bot

# This is a dummy build to get the dependencies cached.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "// dummy file" > src/lib.rs && \
    cargo build --release && \
    rm -r src

# This is the actual build, copy in the rest of the sources
COPY . .
RUN cargo build --release

# Now make the runtime container
FROM debian:buster-slim

RUN apt-get update && apt-get upgrade && apt-get install -y ffmpeg && rm -rf /var/lib/apt/lists/*

COPY --from=builder /bot/target/release/discord_tts_bot /usr/local/bin/discord_tts_bot
COPY Cargo.lock .

CMD ["/usr/local/bin/discord_tts_bot"]
