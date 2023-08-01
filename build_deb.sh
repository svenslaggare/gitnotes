#!/bin/bash
set -eo pipefail

pushd webeditor
./build_js.sh
popd

cargo deb