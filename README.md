# GitNotes
CLI based notes & snippet application powered by Git.

## Features
* Notes stored in Markdown format in a Git repository.
* Edit notes using your favorite editor.
* Notes organized in a virtual file system.
* Possible to run snippets (currently: Python, C++, and Rust) and save output inside notes.
* Ability to search through notes using either content "grep" or note properties.
* Includes an optional web based Markdown editor.

Note: have only been tested on Linux.

## Build
Instructions below is how to build it on a Linux based system (currently Ubuntu based one).

### Binary
Building the `gitnotes` binary is done using:

* Requires cargo (https://rustup.rs/).
* Build with: `cargo build --release`
* Build output in `target/release/gitnotes`

### Web editor
Building the web editor is done using:
* Requires yarn (https://yarnpkg.com/).
* Build with `cd webeditor && ./build_js.sh`

### Debian package
A debian package can be built using the `build_deb.sh` command. This will also include bash auto-completions.

## How to guide

### Creating a repository
First, create a repository using `gitnotes init <name>`. This will create a new git repository in `$HOME/.gitnotes/<name>` and create a configuration file in `$HOME/.gitnotes/config.toml`.

It's also possible to use an existing git repository (at an arbitrary path) using the `gitnotes init <path> --use-existing` command.

### Adding a new note
To add a new note, use `gitnotes add <path>`. This will launch an editor where you can put the initial content of the note. After you are done, a commit will be created.

You can also add tags to a note with `--tags x y` argument.

It is also possible to use pipes as input, `echo Hello | gitnotes add <path>`.

### Editing an existing note
To edit a note, use `gitnotes edit <path>`. This will launch an editor where you can change the content of the note. After saving the changes, a new commit will be created. If you save without making any changes, a commit won't be created.

Other than changing the content, the edit command can be used for adding new tags using `--add-tags` argument or clearing all tags using `--clear-tags` argument.

### Viewing the content of a note
The content of a note can be shown using an editor using the `gitnotes show <path>` command (changes are not stored).

The raw content of the note can be printed using the `gitnotes cat <path>` command. You can view past content using the `--history` argument. Additional filtering such as only showing the code can be done with the `--code` argument.

### Running a snippet
With the `gitnotes run <path>` command, you can run the code blocks that are embedded in the note. If you supply the `--save` arguments, the output is stored in the note.

### Searching for notes
There are multiple ways that we can search for notes. The simplest way is to list the notes using the (virtual) file system with `gitnotes ls` or `gitnotes tree` commands. 

Searching for properties of notes (such as tags or creation date) can be done using the `gitnotes find` command.

Content based searches "grep" can be done with the `gitnotes grep` command. It is also possible to search for past content using the `--history` argument where a git commit spec is used.

### Interactive mode
The interactive mode have some additional features that are not available using the CLi directly such as the ability of combining different operations into one commit:

```
$ gitnotes
> begin
> edit <path 1>
> edit <path 2>
> commit
```

An auto-completion that is aware of the notes that are actually stored in the repository.

### Editor
Any editor can be used to edit notes. The editors that are most preferred are the ones that offer a split code/markdown views such as Visual Studio Code. To minimize the need to use external editors though, a simple web based editor is included with GitNotes. This is used by setting the `editor` config to `web-editor`. It is also possible to run in a standalone fashion using `gitnotes web-editor <path>`.

### Virtual file system
The path used is _virtual_ in the sense that it doesn't affect the actual folder structure (the file path is just a metadata property of the note). All notes also have a numeric ID that can be used to refer to the note instead of the (virtual) path.

### Using real working dir
With `--real` (command line) or `use_real = true` (file), the real working directory will be used when resolving paths. That is, if you change the directory of your terminal, that will reflect in the commands you execute.
The base for this is defined in the config file as the `real_base_dir` (defaults to `$HOME`).