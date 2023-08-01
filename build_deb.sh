#!/bin/bash
set -eo pipefail

pushd webeditor
yarn install --dev
./build_js.sh
popd

cargo deb