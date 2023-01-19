from rust:slim-buster as base
env NETTLE_STATIC=yes \
    CARGO_TERM_PROGRESS_WHEN="always" \
    CARGO_TERM_PROGRESS_WIDTH="80"
workdir /src

run apt-get update && \
    apt-get install -y \
    clang \
    cmake \
    llvm \
    nettle-dev \
    libfreetype6-dev \
    libfontconfig1-dev && \
    rm -rf /var/lib/apt/lists/*

copy rust-toolchain.toml .
run cargo install cargo-chef --locked

# ---

from base as plan

copy . .
run cargo chef prepare --recipe-path recipe.json

# ---

from base as build

copy --from=plan /src/recipe.json .
run cargo chef cook --recipe-path recipe.json --release --no-default-features --features prod

copy . .
run cargo build --release --no-default-features --features prod && \
    cp /src/target/release/rusty-ponyo /

# ---

from debian:buster-slim

run apt-get update && \
    apt-get install -y \
    libfreetype6 \
    libfontconfig1 \
    fonts-noto-cjk && \
    rm -rf /var/lib/apt/lists/*

run echo '<?xml version="1.0"?>\
    <!DOCTYPE fontconfig SYSTEM "fonts.dtd">\
    <fontconfig>\
    <match target="pattern">\
    <test name="family" qual="any"><string>sans-serif</string></test>\
    <edit name="family" mode="prepend" binding="same"><string>Noto Sans CJK JP</string></edit>\
    </match>\
    </fontconfig>' > /etc/fonts/local.conf


copy --from=build /rusty-ponyo /

cmd ["/rusty-ponyo"]
