#!/bin/bash
rm -f static/webeditor.js
yarn webpack --config prod.webpack.config.js