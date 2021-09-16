FROM rust:1.55 as base

RUN apt-get update && \
    apt install -y clang llvm pkg-config nettle-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . /src/

ENV NETTLE_STATIC=yes

RUN --mount=type=cache,target=/root/.cargo/ \
    --mount=type=cache,target=/src/target \
    cargo build --release --no-default-features --features "prod" && \
    cp /src/target/release/rusty-ponyo /


FROM gcr.io/distroless/cc

COPY --from=base /rusty-ponyo /

CMD ["/rusty-ponyo"]
