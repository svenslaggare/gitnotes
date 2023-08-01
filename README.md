# GitNotes
CLI based notes & snippet application powered by Git.

## Features
* Notes stored in markdown format in a Git repository.
* Edit notes using your favorite editor.
* Notes ordered by a virtual file system.
* Possible to run snippets (currently: Python, C++ & Rust) and save output inside notes.
* Ability to search through notes using either content "grep" or note properties.
* Includes an optional web based markdown editor.
  * Run standalone with: `gitnotes web-editor <file>`
  * Use with gitnotes by setting `editor` config to `web-editor`.

Currently only supported on Linux.

## Build
* Requires cargo (https://rustup.rs/).
* Build with: `cargo build --release`
* Build output in `target/release/gitnotes`
