FROM rust:latest
MAINTAINER caleb <calebsmithwoolrich@gmail.com>

RUN echo "Version: 0.1.31"

RUN mkdir -p /app && git clone https://git.opensam.foundation/sam/jupiter /app \
    && cd /app \
    && cargo build --release  \
    && rm -Rf /app/src  /app/target/release/build /app/target/release/deps /app/target/release/examples/ /app/target/release/incremental/ /app/target/release/native
WORKDIR /app/target/release

EXPOSE 9091

CMD ["./jupiter"]