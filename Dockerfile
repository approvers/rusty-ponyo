from rust:slim-buster as base
workdir /src

# copy rust-toolchain.toml .

# create rust toolchain cache layer
run cargo --version

run cargo install cargo-chef

env NETTLE_STATIC=yes \
    HOGWEED_STATIC=yes \
    GMP_STATIC=yes \
    SYSROOT=/dummy

# ---

from base as plan

run --mount=type=bind,target=. cargo chef prepare --recipe-path /recipe.json

# ---

# note to myself: building with alpine to make fully static binary is bad idea.
# it stucks on error like "libclang.so: Dynamic loading not supported".

from base as build
arg FEATURES="discord_client,mongo_db,plot_plotters_static"

run --mount=type=cache,target=/var/lib/apt,sharing=locked \
    --mount=type=cache,target=/var/cache/apt,sharing=locked \
    apt-get update \
 && apt-get install -y \
      wget unzip clang \
      cmake llvm nettle-dev \
      pkg-config fontforge

run --mount=type=bind,source=download_font.sh,target=download_font.sh \
    ./download_font.sh

copy --from=plan /recipe.json .
run --mount=type=cache,target=/src/target/,sharing=locked \
    cargo chef cook \
      --recipe-path recipe.json \
      --release --no-default-features --features ${FEATURES}

copy . .
run --mount=type=cache,target=/src/target/,sharing=locked \
    cargo build --release --no-default-features --features ${FEATURES} \
 && cp ./target/release/rusty-ponyo /

# ---

from gcr.io/distroless/cc-debian11

copy --from=build /rusty-ponyo /

cmd ["/rusty-ponyo"]
