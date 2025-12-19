from rust:slim-trixie as rust-base
workdir /app

run cargo install cargo-chef
run apt update && apt install -y clang llvm pkg-config nettle-dev

# ---

from rust-base as plan
run --mount=type=bind,target=. cargo chef prepare --recipe-path /recipe.json


# ---

from rust-base as build

run apt update && apt install -y git python3 wget unzip fontforge clang

run --mount=type=bind,source=download_font.sh,target=download_font.sh \
    ./download_font.sh

copy --from=plan /recipe.json .
run --mount=type=cache,target=/app/target/,sharing=locked \
    cargo chef cook \
    --recipe-path recipe.json \
    --release --no-default-features --features prod

copy . .
run --mount=type=cache,target=/app/target/,sharing=locked \
    cargo build --release --no-default-features --features prod && \
    cp /app/target/release/rusty-ponyo /

# ---

from debian:trixie-slim

run groupadd -r ponyo && useradd -r -g ponyo ponyo
# run apt update && apt install -y nettle

user ponyo
copy --from=build --chown=ponyo:ponyo /rusty-ponyo .

cmd ["/rusty-ponyo"]
