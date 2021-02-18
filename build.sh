#!/bin/sh

SCRIPT=$(readlink -f "$0")
SCRIPTPATH=$(dirname "$SCRIPT")

cd $SCRIPTPATH/bot/alias/parser
rm parser.h
cbindgen --config cbindgen.toml --output parser.h
cargo clean
cargo build --release

cd $SCRIPTPATH
go build -a
