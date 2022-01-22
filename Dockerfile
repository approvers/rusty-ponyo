FROM rust:slim-buster as base

RUN apt-get update && \
    apt-get install -y \
        clang \
        cmake \
        llvm \
        nettle-dev \
        libfreetype6-dev \
        libfontconfig1-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . /src/

ENV NETTLE_STATIC=yes

RUN --mount=type=cache,target=/root/.cargo/ \
    --mount=type=cache,target=/src/target \
    cargo build --release --no-default-features --features "prod" && \
    cp /src/target/release/rusty-ponyo /


FROM debian:buster-slim

RUN apt-get update && \
    apt-get install -y \
        libfreetype6 \
        libfontconfig1 \
        fonts-noto-cjk && \
    rm -rf /var/lib/apt/lists/*

RUN echo '<?xml version="1.0"?>\
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">\
<fontconfig>\
    <match target="pattern">\
        <test name="family" qual="any"><string>sans-serif</string></test>\
        <edit name="family" mode="prepend" binding="same"><string>Noto Sans CJK JP</string></edit>\
    </match>\
</fontconfig>' > /etc/fonts/local.conf


COPY --from=base /rusty-ponyo /

CMD ["/rusty-ponyo"]
