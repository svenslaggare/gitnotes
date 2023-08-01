#!/bin/bash
pushd webeditor
./build_js.sh
popd

cargo deb