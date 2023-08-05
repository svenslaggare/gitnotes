use std::io::Write;
use std::path::{Path};
use std::process::Stdio;

use crate::command::{CommandError, CommandResult};
use crate::config::Config;
use crate::helpers::io_error;
use crate::model::NOTE_CONTENT_EXT;
use crate::web_editor;
use crate::web_editor::WebEditorConfig;

pub fn launch(config: &Config, path: &Path) -> CommandResult<()> {
    let mut editor_command = std::process::Command::new(&config.editor);
    match config.editor.as_str() {
        "code" | "gedit" | "xed" => { editor_command.arg("--wait"); },
        "web-editor" => {
            web_editor::launch_sync(WebEditorConfig::default(), path);
            return Ok(());
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
        Ok(())
    } else {
        Err(CommandError::SubProcess(io_error(format!("Non successful result: {}", result.code().unwrap_or(1)))))
    }
}

pub fn launch_with_content(config: &Config, content: &str) -> CommandResult<()> {
    let ext = ".".to_owned() + NOTE_CONTENT_EXT;
    let temp_file = tempfile::Builder::new()
        .suffix(&ext)
        .tempfile()?;
    temp_file.as_file().write_all(content.as_bytes())?;
    launch(config, temp_file.path())
}