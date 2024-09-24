from archlinux as arch-base
workdir /app

run echo 'Server = https://mirrors.cat.net/archlinux/$repo/os/$arch' > /etc/pacman.d/mirrorlist
run --mount=type=cache,target=/var/cache/pacman,sharing=locked \
    pacman -Syu --noconfirm --needed base-devel git

# ---

from arch-base as gmp

run useradd kawak -m
user kawak
run cd \
 && git clone https://gitlab.archlinux.org/archlinux/packaging/packages/gmp.git \
 && cd gmp \
 && echo 'options=(!strip staticlibs)' >> PKGBUILD \
 && MAKEFLAGS="-j$(nproc)" makepkg --skippgpcheck --nocheck --noconfirm
user root
run mv /home/kawak/gmp/gmp-*.pkg.* /gmp

# ---

from arch-base as nettle

run useradd kawak -m
user kawak
run cd \
 && git clone https://gitlab.archlinux.org/archlinux/packaging/packages/nettle.git \
 && cd nettle \
 && echo 'options=(!strip staticlibs)' >> PKGBUILD \
 && sed -i 's/--disable-static//g' PKGBUILD \
 && MAKEFLAGS="-j$(nproc)" makepkg --skippgpcheck --nocheck --noconfirm
user root
run mv /home/kawak/nettle/nettle-*.pkg.* /nettle

# ---

from arch-base as rust-base
run --mount=type=cache,target=/var/cache/pacman,sharing=locked \
    pacman -Sy --noconfirm --needed rustup
run rustup default stable && cargo --version
run cargo install cargo-chef

# ---

from rust-base as plan
run --mount=type=bind,target=. cargo chef prepare --recipe-path /recipe.json

# ---

from rust-base as build

run --mount=type=cache,target=/var/cache/pacman,sharing=locked \
    pacman -Sy --noconfirm --needed git python3 wget unzip fontforge clang

run --mount=type=bind,source=download_font.sh,target=download_font.sh \
    ./download_font.sh

copy --from=gmp /gmp .
copy --from=nettle /nettle .
run pacman -U --noconfirm gmp nettle

env NETTLE_STATIC=yes \
    HOGWEED_STATIC=yes \
    GMP_STATIC=yes \
    SYSROOT=/dummy

copy --from=plan /recipe.json .
run --mount=type=cache,target=/src/target/,sharing=locked \
   cargo chef cook \
      --recipe-path recipe.json \
      --release --no-default-features --features prod

# 謎 of 謎
# なんで nettle-sys の静的リンクだとシンボルが足りない的なのが出るのかわからん
# 多分 .rlib に静的リンクしようとしてできてない？
run \
    echo 'fn main() {' > build.rs \
 && echo '  println!("cargo::rustc-link-arg=/usr/lib/libhogweed.a");' >> build.rs \
 && echo '  println!("cargo::rustc-link-arg=/usr/lib/libnettle.a");' >> build.rs \
 && echo '  println!("cargo::rustc-link-arg=/usr/lib/libgmp.a");' >> build.rs \
 && echo '}' >> build.rs

copy . .
run --mount=type=cache,target=/src/target/,sharing=locked \
   cargo build --release --no-default-features --features prod

# ---

from gcr.io/distroless/static-debian11

copy --from=build /usr/lib/libc.so.6 /usr/lib/libm.so.6 /usr/lib/libgcc_s.so.1 /usr/lib
copy --from=build /usr/lib64/ld-linux-x86-64.so.2 /lib64/
copy --from=build /app/target/release/rusty-ponyo /

cmd ["/rusty-ponyo"]
