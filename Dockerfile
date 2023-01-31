from rust:slim-buster as base
env NETTLE_STATIC=yes \
    CARGO_TERM_PROGRESS_WHEN="always" \
    CARGO_TERM_PROGRESS_WIDTH="80"
workdir /src

copy rust-toolchain.toml .
run cargo install cargo-chef --locked

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
