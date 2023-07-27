use std::io::{Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use fnv::FnvHashMap;

use thiserror::Error;

use tempfile::NamedTempFile;

pub type SnippetResult<T> = Result<T, SnippetError>;

#[derive(Error, Debug)]
pub enum SnippetError {
    #[error("No runner found for '{0}'")]
    RunnerNotFound(String),

    #[error("Run command error: {0}")]
    RunCommand(std::io::Error),

    #[error("Failed to compile (see console output)")]
    Compiler,

    #[error("Execution error: {status}")]
    Execution {
        status: ExitStatus,
        output: String
    },

    #[error("I/O error: {0}")]
    IO(std::io::Error)
}

impl From<std::io::Error> for SnippetError {
    fn from(err: std::io::Error) -> Self {
        SnippetError::IO(err)
    }
}

pub struct SnipperRunnerManger {
    runners: FnvHashMap<String, Box<dyn SnippetRunner>>
}

impl SnipperRunnerManger {
    pub fn new() -> SnipperRunnerManger {
        SnipperRunnerManger {
            runners: FnvHashMap::default()
        }
    }

    pub fn add_runner(&mut self, name: &str, runner: Box<dyn SnippetRunner>) {
        self.runners.insert(name.to_owned(), runner);
    }

    pub fn run(&self, name: &str, source_code: &str) -> SnippetResult<String> {
        let runner = self.runners.get(name).ok_or_else(|| SnippetError::RunnerNotFound(name.to_owned()))?;
        runner.run(source_code)
    }
}

impl Default for SnipperRunnerManger {
    fn default() -> Self {
        let mut manager = SnipperRunnerManger::new();
        manager.add_runner("python", Box::new(PythonSnippetRunner::default()));
        manager.add_runner("cpp", Box::new(CppSnippetRunner::default()));
        manager
    }
}

pub trait SnippetRunner {
    fn run(&self, source_code: &str) -> SnippetResult<String>;
}

pub struct PythonSnippetRunner {
    executable: PathBuf
}

impl PythonSnippetRunner {
    pub fn new(executable: PathBuf) -> PythonSnippetRunner {
        PythonSnippetRunner {
            executable
        }
    }
}

impl Default for PythonSnippetRunner {
    fn default() -> Self {
        PythonSnippetRunner::new(Path::new("python3").to_owned())
    }
}

impl SnippetRunner for PythonSnippetRunner {
    fn run(&self, source_code: &str) -> SnippetResult<String> {
        let mut source_code_file = NamedTempFile::new()?;
        source_code_file.write_all(source_code.as_bytes())?;

        run_and_capture(Command::new(&self.executable).arg(source_code_file.path()))
    }
}

pub struct CppSnippetRunner {
    compiler_executable: PathBuf,
    compiler_flags: Vec<String>
}

impl CppSnippetRunner {
    pub fn new(compiler_executable: PathBuf,
               compiler_flags: Vec<String>) -> CppSnippetRunner {
        CppSnippetRunner {
            compiler_executable,
            compiler_flags
        }
    }
}

impl Default for CppSnippetRunner {
    fn default() -> Self {
        CppSnippetRunner::new(
            Path::new("c++").to_owned(),
            vec!["-std=c++14".to_owned()]
        )
    }
}

impl SnippetRunner for CppSnippetRunner {
    fn run(&self, source_code: &str) -> SnippetResult<String> {
        let mut source_code_file = tempfile::Builder::new()
            .suffix(".cpp")
            .tempfile()?;
        source_code_file.write_all(source_code.as_bytes())?;

        let compiled_executable = {
            tempfile::Builder::new()
                .suffix(".out")
                .tempfile()?
                .path().to_path_buf()
        };
        let _delete_compiled_executable = DeleteFileGuard::new(&compiled_executable);

        let output = Command::new(&self.compiler_executable)
            .args(self.compiler_flags.iter())
            .arg(source_code_file.path())
            .arg("-o")
            .arg(&compiled_executable)
            .spawn()?
            .wait()?;

        if !output.success() {
            return Err(SnippetError::Compiler);
        }

        run_and_capture(&mut Command::new(&compiled_executable))
    }
}

fn run_and_capture(command: &mut Command) -> SnippetResult<String> {
    let output = unsafe {
        command
            .pre_exec(|| { libc::dup2(1, 2); Ok(()) })
            .output()
            .map_err(|err| SnippetError::RunCommand(err))
    }?;

    let stdout = String::from_utf8(output.stdout).unwrap();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(
            SnippetError::Execution {
                status: output.status,
                output: stdout
            }
        )
    }
}

struct DeleteFileGuard {
    path: PathBuf
}

impl DeleteFileGuard {
    pub fn new(path: &Path) -> DeleteFileGuard {
        DeleteFileGuard {
            path: path.to_path_buf()
        }
    }
}

impl Drop for DeleteFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn test_python_success1() {
    let runner = PythonSnippetRunner::default();
    let result = runner.run(r#"
xs = list(range(0, 10))
print([x * x for x in xs])
    "#);

    assert_eq!("[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]\n".to_owned(), result.unwrap());
}

#[test]
fn test_python_fail1() {
    let runner = PythonSnippetRunner::default();
    let result = runner.run(r#"
import wololo
xs = list(range(0, 10))
print([x * x for x in xs])
    "#);

    assert_eq!(false, result.is_ok());

    if let SnippetError::Execution { status, output } = result.err().unwrap() {
        assert!(!status.success());
        assert!(output.contains("Traceback"));
    } else {
        assert!(false, "Expected 'Execution' error.");
    }
}

#[test]
fn test_cpp_success1() {
    let runner = CppSnippetRunner::default();
    let result = runner.run(r#"
#include <iostream>
int main() {
    std::cout << "Hello, World!" << std::endl;
}
    "#);

    assert_eq!("Hello, World!\n".to_owned(), result.unwrap());
}

#[test]
fn test_cpp_fail1() {
    let runner = CppSnippetRunner::default();
    let result = runner.run(r#"
int main() {
    std::cout << "Hello, World!" << std::endl;
}
    "#);

    if let SnippetError::Compiler = result.err().unwrap() {
        assert!(true);
    } else {
        assert!(false, "Expected 'Compiler' error.");
    }
}

#[test]
fn test_manager_success1() {
    let manager = SnipperRunnerManger::default();
    let result = manager.run("python", r#"
xs = list(range(0, 10))
print([x * x for x in xs])
    "#);

    assert_eq!("[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]\n".to_owned(), result.unwrap());
}