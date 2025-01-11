#!/bin/bash
set -eo pipefail
cargo run --release generate-completions
cargo deb
sudo dpkg -i target/debian/gitnotes_0.1.0_amd64.deb