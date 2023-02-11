FROM rust:slim-buster as base
ENV CARGO_TERM_PROGRESS_WHEN="always" \
    CARGO_TERM_PROGRESS_WIDTH="80"
WORKDIR /src

COPY rust-toolchain.toml .
RUN cargo install cargo-chef --locked

# workaround for https://gitlab.com/sequoia-pgp/nettle-sys/-/issues/16
ENV NETTLE_STATIC=yes \
    HOGWEED_STATIC=yes \
    GMP_STATIC=yes \
    SYSROOT=/dummy

# ---

FROM base as plan

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---

# note to myself: building with alpine to make fully static binary is bad idea.
# it stucks on error like "libclang.so: Dynamic loading not supported".

FROM base as build
ARG FEATURES="discord_client,mongo_db,plot_plotters_static"

RUN apt-get update \
    && apt-get install -y \
    wget unzip clang \
    cmake llvm nettle-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY --from=plan /src/download_font.sh .
RUN ./download_font.sh

COPY --from=plan /src/recipe.json .
RUN cargo chef cook \
    --recipe-path recipe.json \
    --release --no-default-features --features ${FEATURES}

COPY . .
RUN cargo build --release --no-default-features --features ${FEATURES}

# ---

FROM gcr.io/distroless/cc-debian11

COPY --from=build /src/target/release/rusty-ponyo /
COPY --from=build /src/OFL.txt /

CMD ["/rusty-ponyo"]
