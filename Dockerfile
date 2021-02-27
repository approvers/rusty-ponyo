FROM rust:alpine as base

RUN apk add --no-cache musl-dev

RUN mkdir /src
COPY . /src/

WORKDIR /src
RUN cargo build --release --no-default-features --features "prod"


FROM alpine

COPY --from=base /src/target/release/rusty-ponyo /usr/local/bin

CMD ["/usr/local/bin/rusty-ponyo"]
