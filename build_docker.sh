#!/bin/sh
cargo build -p peershare --target x86_64-unknown-linux-musl --release
rm -r target/docker/
mkdir -p target/docker/
mv target/x86_64-unknown-linux-musl/release/peershare target/docker/
docker build target/docker/ -f Dockerfile -t peershare/peershare