const path = require('path');

module.exports = {
    mode: "production",
    entry: './scripts/webeditor.js',
    output: {
        path: path.resolve(__dirname, 'static'),
        filename: 'webeditor.js'
    }
};