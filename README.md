# GitNotes
CLI based notes & snippet application powered by Git.

## Features
* Notes stored in Markdown format in a Git repository.
* Edit notes using your favorite editor.
* Notes ordered by a virtual file system.
* Possible to run snippets (currently: Python, C++ & Rust) and save output inside notes.
* Ability to search through notes using either content "grep" or note properties.
* Includes an optional web based Markdown editor.
  * Use with gitnotes by setting `editor` config to `web-editor`.
  * Run standalone with: `gitnotes web-editor <file>`

Currently only supported on Linux.

## Build
* Requires cargo (https://rustup.rs/).
* Build with: `cargo build --release`
* Build output in `target/release/gitnotes`

## How to guide

### Creating repository.
First, create a repository using `gitnotes init <name>`. This will create a new git repository in `$HOME/.gitnotes/<name>` and create a configuration file in `$HOME/.gitnotes/config.toml`.

It's also possible to use an existing git repository (at arbitrary path) using `gitnotes init <path> --use-existing`

### Adding a new note
To add a new, use `gitnotes add <path>`. This will launch an editor where you can put the initial content of the note. After you are done, a commit will be created.
Remember, the content of a note is just markdown.

You can also add tags to a note with `--tags x y` argument.

It is also possible to pipe input instead of launching an editor.

### Editing an existing note
To edit a note, use `gitnotes edit <path>`. This will launch an editor where you can change the content of the note.

### Running a snippet
With the `gitnotes run <path>` command, you can run the code blocks that are embedded in the note. If you supply the `--save` arguments, the output is stored in the note.

### Searching for notes
Notes can be searched through in different way. The simplest way is to use the (virtual) file system using `gitnotes ls` or `gitnotes tree`. 

More advanced searching can be done with `gitnotes find` for searching based on properties or `gitnotes grep` to search based on content.

As the data is stored in a git repository, it is possible to grep using historic content as well.

### Interactive mode
The interactive mode removes the need to start new processes of `gitnotes` all the time, but also allows you to combine several changes into one commit:

```
$ gitnotes
> begin
> edit <path 1>
> edit <path 2>
> commit
```

With these command, we will first enter interactive mode, then start a new commit that will combine the edits to two notes into one commit.

### Virtual file system
The path used is _virtual_ in the sense that it doesn't affect the actual folder structure (the file path is just a metadata property). All notes also have a numeric ID that can be used to refer to the note instead of the path.

The purpose is to allow different structures depending on need of users. Example: folder structure that is only based on creation date (currently previewed by `gitnotes tree --date` command).
