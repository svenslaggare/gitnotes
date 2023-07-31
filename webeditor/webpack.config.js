const path = require('path');

module.exports = {
    mode: "development",
    watch: true,
    entry: './scripts/webeditor.js',
    output: {
        path: path.resolve(__dirname, 'static'),
        filename: 'webeditor.js'
    }
};