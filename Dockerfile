from rust:slim-buster as base
workdir /src

copy rust-toolchain.toml .

# create rust toolchain cache layer
run cargo --version

run cargo install cargo-chef

# workaround for https://gitlab.com/sequoia-pgp/nettle-sys/-/issues/16
env NETTLE_STATIC=yes \
    HOGWEED_STATIC=yes \
    GMP_STATIC=yes \
    SYSROOT=/dummy

# ---

from base as plan

copy . .
run cargo chef prepare --recipe-path recipe.json

# ---

# note to myself: building with alpine to make fully static binary is bad idea.
# it stucks on error like "libclang.so: Dynamic loading not supported".

from base as build
arg FEATURES="discord_client,mongo_db,plot_plotters_static"

run apt-get update \
    && apt-get install -y \
       wget unzip clang \
       cmake llvm nettle-dev \
       pkg-config \
       libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

copy --from=plan /src/download_font.sh .
run ./download_font.sh

copy --from=plan /src/recipe.json .
run cargo chef cook \
    --recipe-path recipe.json \
    --release --no-default-features --features ${FEATURES}

copy . .
run cargo build --release --no-default-features --features ${FEATURES}

# ---

from gcr.io/distroless/cc-debian11

copy --from=build /src/target/release/rusty-ponyo /
copy --from=build /src/OFL.txt /

cmd ["/rusty-ponyo"]
