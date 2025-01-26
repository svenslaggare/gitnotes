use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::command::{CommandError, CommandResult};
use crate::config::Config;
use crate::helpers::io_error;
use crate::model::NOTE_CONTENT_EXT;
use crate::web_editor;
use crate::web_editor::{AccessMode, WebEditorConfig, WebEditorInput};

pub struct EditorOutput {
    pub added_resources: Vec<PathBuf>
}

impl Default for EditorOutput {
    fn default() -> Self {
        EditorOutput {
            added_resources: Vec::new(),
        }
    }
}

pub fn launch(
    config: &Config,
    path: &Path,
    display_path: Option<&Path>,
    access_mode: AccessMode
) -> CommandResult<EditorOutput> {
    let mut editor_command = std::process::Command::new(&config.editor);
    match config.editor.as_str() {
        "code" | "gedit" | "xed" => { editor_command.arg("--wait"); },
        "web-editor" => {
            let mut web_config = WebEditorConfig::default();
            web_config.access_mode = access_mode;
            web_config.snippet_config = config.snippet.clone();

            return Ok(
                web_editor::launch_sync(
                    web_config,
                    WebEditorInput {
                        path: path.to_owned(),
                        display_path: display_path.map(|x| x.to_owned()),
                        repository_path: Some(config.repository.clone())
                    }
                )
            );
        }
        _ => {}
    }

    let mut result = editor_command
        .arg(path)
        .stdin(Stdio::inherit())
        .spawn()
        .map_err(|err| CommandError::SubProcess(err))?;

    let result = result.wait().map_err(|err| CommandError::SubProcess(err))?;
    if result.success() {
        Ok(EditorOutput::default())
    } else {
        Err(CommandError::SubProcess(io_error(format!("Non successful result: {}", result.code().unwrap_or(1)))))
    }
}

pub fn launch_with_content(
    config: &Config,
    content: &str,
    display_path: Option<&Path>,
    access_mode: AccessMode
) -> CommandResult<EditorOutput> {
    let ext = ".".to_owned() + NOTE_CONTENT_EXT;
    let temp_file = tempfile::Builder::new()
        .suffix(&ext)
        .tempfile()?;
    temp_file.as_file().write_all(content.as_bytes())?;
    launch(config, temp_file.path(), display_path, access_mode)
}