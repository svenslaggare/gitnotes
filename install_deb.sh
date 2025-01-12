#!/bin/bash
set -eo pipefail
cargo run --release generate-completions
cargo deb
sudo dpkg -i target/debian/gitnotes_*_amd64.deb
