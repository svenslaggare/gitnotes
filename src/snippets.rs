use std::any::Any;
use std::io::{Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use serde::{Serialize, Deserialize};
use fnv::FnvHashMap;
use thiserror::Error;

use crate::config::SnippetFileConfig;

pub type SnippetResult<T> = Result<T, SnippetError>;

#[derive(Error, Debug)]
pub enum SnippetError {
    #[error("No runner found for '{0}'")]
    RunnerNotFound(String),

    #[error("The configuration type is not valid for this runner")]
    InvalidConfigType,

    #[error("{0}")]
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

pub struct SnippetRunnerManger {
    runners: FnvHashMap<String, Box<dyn SnippetRunner + Send + Sync>>
}

impl SnippetRunnerManger {
    pub fn new() -> SnippetRunnerManger {
        SnippetRunnerManger {
            runners: FnvHashMap::default()
        }
    }

    pub fn from_config(config: Option<&SnippetFileConfig>) -> SnippetResult<SnippetRunnerManger> {
        let mut snippet_runner_manager = SnippetRunnerManger::default();
        if let Some(config) = config {
            snippet_runner_manager.apply_config(config)?;
        }

        Ok(snippet_runner_manager)
    }

    pub fn add_runner(&mut self, name: &str, runner: Box<dyn SnippetRunner + Send + Sync>) {
        self.runners.insert(name.to_owned(), runner);
    }

    pub fn run(&self, name: &str, source_code: &str) -> SnippetResult<String> {
        let runner = self.runners.get(name).ok_or_else(|| SnippetError::RunnerNotFound(name.to_owned()))?;
        runner.run(source_code)
    }

    pub fn apply_config(&mut self, file_config: &SnippetFileConfig) -> SnippetResult<()> {
        self.change_config_opt("python", file_config.python.as_ref())?;
        self.change_config_opt("cpp", file_config.python.as_ref())?;
        self.change_config_opt("rust", file_config.rust.as_ref())?;
        Ok(())
    }

    fn change_config_opt<T: 'static>(&mut self, name: &str, config: Option<&T>) -> SnippetResult<()> {
        if let Some(config) = config {
            self.change_config(name, config)?;
        }

        Ok(())
    }

    pub fn change_config(&mut self, name: &str, config: &dyn Any) -> SnippetResult<()> {
        let runner = self.runners.get_mut(name).ok_or_else(|| SnippetError::RunnerNotFound(name.to_owned()))?;
        runner.change_config(config)?;
        Ok(())
    }
}

impl Default for SnippetRunnerManger {
    fn default() -> Self {
        let mut manager = SnippetRunnerManger::new();
        manager.add_runner("python", Box::new(PythonSnippetRunner::default()));
        manager.add_runner("cpp", Box::new(CppSnippetRunner::default()));
        manager.add_runner("rust", Box::new(RustSnippetRunner::default()));
        manager
    }
}

pub trait SnippetRunner {
    fn run(&self, source_code: &str) -> SnippetResult<String>;

    fn change_config(&mut self, config: &dyn Any) -> SnippetResult<()>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PythonSnippetRunnerConfig {
    pub executable: PathBuf
}

pub struct PythonSnippetRunner {
    config: PythonSnippetRunnerConfig
}

impl PythonSnippetRunner {
    pub fn new(config: PythonSnippetRunnerConfig) -> PythonSnippetRunner {
        PythonSnippetRunner {
            config
        }
    }
}

impl Default for PythonSnippetRunner {
    fn default() -> Self {
        PythonSnippetRunner::new(
            PythonSnippetRunnerConfig {
                executable: Path::new("python3").to_owned(),
            }
        )
    }
}

impl SnippetRunner for PythonSnippetRunner {
    fn run(&self, source_code: &str) -> SnippetResult<String> {
        let mut source_code_file = tempfile::Builder::new()
            .suffix(".py")
            .tempfile()?;
        source_code_file.write_all(source_code.as_bytes())?;

        run_and_capture(Command::new(&self.config.executable).arg(source_code_file.path()))
    }

    fn change_config(&mut self, config: &dyn Any) -> SnippetResult<()> {
        if let Some(config) = config.downcast_ref::<PythonSnippetRunnerConfig>() {
            self.config = config.clone();
            Ok(())
        } else {
            Err(SnippetError::InvalidConfigType)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CppSnippetRunnerConfig {
    pub compiler_executable: PathBuf,
    pub compiler_flags: Vec<String>
}

pub struct CppSnippetRunner {
    config: CppSnippetRunnerConfig
}

impl CppSnippetRunner {
    pub fn new(config: CppSnippetRunnerConfig) -> CppSnippetRunner {
        CppSnippetRunner {
            config
        }
    }
}

impl Default for CppSnippetRunner {
    fn default() -> Self {
        CppSnippetRunner::new(
            CppSnippetRunnerConfig {
                compiler_executable: Path::new("c++").to_owned(),
                compiler_flags: vec!["-std=c++14".to_owned()],
            }
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

        let output = Command::new(&self.config.compiler_executable)
            .args(self.config.compiler_flags.iter())
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

    fn change_config(&mut self, config: &dyn Any) -> SnippetResult<()> {
        if let Some(config) = config.downcast_ref::<CppSnippetRunnerConfig>() {
            self.config = config.clone();
            Ok(())
        } else {
            Err(SnippetError::InvalidConfigType)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustSnippetRunnerConfig {
    pub compiler_executable: PathBuf,
    pub compiler_flags: Vec<String>
}

pub struct RustSnippetRunner {
    config: RustSnippetRunnerConfig
}

impl RustSnippetRunner {
    pub fn new(config: RustSnippetRunnerConfig) -> RustSnippetRunner {
        RustSnippetRunner {
            config
        }
    }
}

impl Default for RustSnippetRunner {
    fn default() -> Self {
        RustSnippetRunner::new(
            RustSnippetRunnerConfig {
                compiler_executable: Path::new("rustc").to_owned(),
                compiler_flags: vec![
                    "--edition".to_owned(), "2021".to_owned()
                ]
            }
        )
    }
}

impl SnippetRunner for RustSnippetRunner {
    fn run(&self, source_code: &str) -> SnippetResult<String> {
        let mut source_code_file = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()?;
        source_code_file.write_all(source_code.as_bytes())?;

        let compiled_executable = {
            tempfile::Builder::new()
                .suffix(".out")
                .tempfile()?
                .path().to_path_buf()
        };
        let _delete_compiled_executable = DeleteFileGuard::new(&compiled_executable);

        let output = Command::new(&self.config.compiler_executable)
            .args(self.config.compiler_flags.iter())
            .arg(source_code_file.path())
            .args(["--crate-name", "snippet"])
            .arg("-o")
            .arg(&compiled_executable)
            .spawn()?
            .wait()?;

        if !output.success() {
            return Err(SnippetError::Compiler);
        }

        run_and_capture(&mut Command::new(&compiled_executable))
    }

    fn change_config(&mut self, config: &dyn Any) -> SnippetResult<()> {
        if let Some(config) = config.downcast_ref::<RustSnippetRunnerConfig>() {
            self.config = config.clone();
            Ok(())
        } else {
            Err(SnippetError::InvalidConfigType)
        }
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
fn test_manager_success1() {
    let manager = SnippetRunnerManger::default();
    let result = manager.run("python", r#"
xs = list(range(0, 10))
print([x * x for x in xs])
    "#);

    assert_eq!("[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]\n".to_owned(), result.unwrap());
}

#[test]
fn test_manager_success2() {
    let manager = SnippetRunnerManger::default();
    let result = manager.run("cpp", r#"
#include <iostream>
int main() {
    std::cout << "Hello, World!" << std::endl;
}
    "#);

    assert_eq!("Hello, World!\n".to_owned(), result.unwrap());
}

#[test]
fn test_manager_success3() {
    let manager = SnippetRunnerManger::default();
    let result = manager.run("rust", r#"
fn main() {
    println!("Hello, World!");
}
    "#);

    assert_eq!("Hello, World!\n".to_owned(), result.unwrap());
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
fn test_python_change_config1() {
    let mut runner = PythonSnippetRunner::default();
    runner.change_config(&PythonSnippetRunnerConfig {
        executable: Path::new("python2").to_path_buf(),
    }).unwrap();

    assert_eq!(Path::new("python2"), runner.config.executable);
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
fn test_rust_success1() {
    let runner = RustSnippetRunner::default();
    let result = runner.run(r#"
fn main() {
    println!("Hello, World!");
}
    "#);

    assert_eq!("Hello, World!\n".to_owned(), result.unwrap());
}

#[test]
fn test_rust_success2() {
    let mut runner = RustSnippetRunner::default();
    runner.config.compiler_flags = vec!["--edition".to_owned(), "2021".to_owned(), "-O".to_owned()];
    let result = runner.run(r#"
fn main() {
    println!("Hello, World!");
}
    "#);

    assert_eq!("Hello, World!\n".to_owned(), result.unwrap());
}
