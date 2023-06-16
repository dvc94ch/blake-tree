#!/bin/sh
cargo build -p peershare --target x86_64-unknown-linux-musl --release
rm -r target/release/peershare
mkdir -p target/release/peershare/bin
mv target/x86_64-unknown-linux-musl/release/peershare target/release/peershare/bin
cp -r static target/release/peershare/
docker build target/release/peershare -f Dockerfile -t peershare/peershare