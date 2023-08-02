# GitNotes
CLI based notes & snippet application powered by Git.

## Features
* Notes stored in Markdown format in a Git repository.
* Edit notes using your favorite editor.
* Notes ordered by a virtual file system.
* Possible to run snippets (currently: Python, C++ & Rust) and save output inside notes.
* Ability to search through notes using either content "grep" or note properties.
* Includes an optional web based Markdown editor.

## Build
* Currently only supported on Linux.
* Requires cargo (https://rustup.rs/).
* Build with: `cargo build --release`
* Build output in `target/release/gitnotes`

## How to guide

### Creating a repository
First, create a repository using `gitnotes init <name>`. This will create a new git repository in `$HOME/.gitnotes/<name>` and create a configuration file in `$HOME/.gitnotes/config.toml`.

It's also possible to use an existing git repository (at an arbitrary path) using the `gitnotes init <path> --use-existing` command.

### Adding a new note
To add a new note, use `gitnotes add <path>`. This will launch an editor where you can put the initial content of the note. After you are done, a commit will be created.

You can also add tags to a note with `--tags x y` argument.

It is also possible to use pipes as input, `echo Hello | gitnotes add <path>`.

### Editing an existing note
To edit a note, use `gitnotes edit <path>`. This will launch an editor where you can change the content of the note. After saving the changes, a new commit will be created. 

### Viewing content of a note
To view the content of a note, the `gitnotes show <path>` command can be used. The raw content of the note can be printed using the `gitnotes cat <path>` command. You can view past content using the `--git` argument or only view code/output using `--code` & `--output` options.

### Running a snippet
With the `gitnotes run <path>` command, you can run the code blocks that are embedded in the note. If you supply the `--save` arguments, the output is stored in the note.

### Searching for notes
Notes can be searched through in different way. The simplest way is to use the (virtual) file system using `gitnotes ls` or `gitnotes tree`. 

More advanced searching can be done with `gitnotes find` for searching based on properties or `gitnotes grep` to search based on content.

As the data is stored in a git repository, it is possible to grep for past content as well using the `--history` argument.

### Interactive mode
The interactive mode allows some new features, such as the ability of combining different operations into one commit:

```
$ gitnotes
> begin
> edit <path 1>
> edit <path 2>
> commit
```

It also supports better auto-completion that is aware of the notes inside the repository.

### Editor
Any editor can be used to edit notes. The editors that are most preferred are the ones that offer split code/markdown views such as vscode. To aid in this regard, a web based such editor is included with GitNotes.
This is used by setting the `editor` config to `web-editor`. It is also possible to run in a standalone fashion using `gitnotes web-editor <path>`.


### Virtual file system
The path used is _virtual_ in the sense that it doesn't affect the actual folder structure (the file path is just a metadata property of the note). All notes also have a numeric ID that can be used to refer to the note instead of the (virtual) path.

