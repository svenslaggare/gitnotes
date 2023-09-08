use std::path::Path;

use crate::app::{App, AppError, InputCommand};
use crate::command::{Command, CommandError, CommandInterpreter};
use crate::config::{Config, FileConfig};

#[test]
fn test_add() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec![],
            content: note_content.clone()
        },
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    assert_eq!(vec!["snippet".to_owned(), "python".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);
}

#[test]
fn test_add_with_editor() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let mut config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    config.allow_stdin = false;
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();

    let note_content_clone = note_content.clone();
    let mut app = App::with_custom(config, move |config, repository| {
        CommandInterpreter::with_launch_editor(
            config,
            repository,
            Box::new(move |_, path| {
                std::fs::write(path, &note_content_clone).map_err(|err| CommandError::IO(err))
            })
        )
    }).unwrap();

    app.run(InputCommand::Add {
        path: note_path.to_path_buf(),
        tags: vec![],
    }).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    assert_eq!(vec!["snippet".to_owned(), "python".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);
}

#[test]
fn test_run_snippet() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();

    let note_content_output = r#"Hello, World\!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```

``` output
[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]
```
"#.to_string();

    let note_content2 = r#"Hello, World!

``` python
xs = list(range(0, 11))
print([x * x for x in xs])
```

``` output
[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]
```
"#.to_string();

    let note_content_output2 = r#"Hello, World\!

``` python
xs = list(range(0, 11))
print([x * x for x in xs])
```

``` output
[0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::RunSnippet { path: note_path.to_owned(), save_output: true }).unwrap();
    assert_eq!(note_content_output, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());

    app.execute_commands(app.create_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec![],
            content: note_content2.clone()
        }
    ])).unwrap();
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(3, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::RunSnippet { path: note_path.to_owned(), save_output: true }).unwrap();
    assert_eq!(note_content_output2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(4, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_move() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_path2 = Path::new("2023/07/01/sample");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned(), force: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_move_to_existing1() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_path2 = Path::new("2023/07/01/sample");
    let note_content = "Hello, World #1".to_owned();
    let note_content2 = "Hello, World #2".to_owned();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: note_path2.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content2.clone()
        }
    ]).unwrap();
    let note_id = app.note_metadata_storage().unwrap().get_id(note_path).unwrap();
    let note_id2 = app.note_metadata_storage().unwrap().get_id(note_path2).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    let err = app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned(), force: false }).err().unwrap();
    if let AppError::Command(CommandError::NoteExistsAtDestination(err_path)) = err {
        assert_eq!(note_path2, err_path);
        assert_eq!(note_id, app.note_metadata_storage().unwrap().get_id(note_path).unwrap());
        assert_eq!(note_id2, app.note_metadata_storage().unwrap().get_id(note_path2).unwrap());
    } else {
        assert!(false, "Expected 'NoteAtDestination' error");
    }
}

#[test]
fn test_move_to_existing2() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_path2 = Path::new("2023/07/01/sample");
    let note_content = "Hello, World #1".to_owned();
    let note_content2 = "Hello, World #2".to_owned();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: note_path2.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content2.clone()
        }
    ]).unwrap();
    let note_id = app.note_metadata_storage().unwrap().get_id(note_path).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned(), force: true }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(note_id, app.note_metadata_storage().unwrap().get(note_path2).unwrap().id);
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_move_dir1() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note1_path = Path::new("2023/07/sample");
    let note1_path2 = Path::new("2024/07/sample");
    let note1_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let note2_path = Path::new("2023/07/sample2");
    let note2_path2 = Path::new("2024/07/sample2");
    let note2_content = r#"Hello, My World!

``` python
import numpy as np
print(np.square(np.arange(0, 15)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note1_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note1_content.clone()
        },
        Command::AddNoteWithContent {
            path: note2_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note2_content.clone()
        }
    ]).unwrap();
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: Path::new("2023").to_path_buf(), destination: Path::new("2024").to_path_buf(), force: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note1_path).is_ok());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note2_path).is_ok());
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path2).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_move_dir2() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note1_path = Path::new("2023/07/sample");
    let note1_path2 = Path::new("2023/07/07/sample");
    let note1_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let note2_path = Path::new("2023/07/sample2");
    let note2_path2 = Path::new("2023/07/07/sample2");
    let note2_content = r#"Hello, My World!

``` python
import numpy as np
print(np.square(np.arange(0, 15)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note1_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note1_content.clone()
        },
        Command::AddNoteWithContent {
            path: note2_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note2_content.clone()
        }
    ]).unwrap();
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: Path::new("2023").to_path_buf(), destination: Path::new("2023/07").to_path_buf(), force: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note1_path).is_ok());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note2_path).is_ok());
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path2).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_move_dir_to_existing1() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note1_path = Path::new("2023/07/test1");
    let note1_content = "Test1".to_owned();

    let note2_path = Path::new("2023/07/test2");
    let note2_content = "Test2".to_owned();

    let note3_path = Path::new("2024/07/test2");
    let note3_content = "Test3".to_owned();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note1_path.to_path_buf(),
            tags: vec![],
            content: note1_content.clone()
        },
        Command::AddNoteWithContent {
            path: note2_path.to_path_buf(),
            tags: vec![],
            content: note2_content.clone()
        },
        Command::AddNoteWithContent {
            path: note3_path.to_path_buf(),
            tags: vec![],
            content: note3_content.clone()
        }
    ]).unwrap();
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path).unwrap());
    assert_eq!(note3_content, app.note_metadata_storage().unwrap().get_content(note3_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    let note_id1 = app.note_metadata_storage().unwrap().get_id(note1_path).unwrap();
    let note_id2 = app.note_metadata_storage().unwrap().get_id(note2_path).unwrap();
    let note_id3 = app.note_metadata_storage().unwrap().get_id(note3_path).unwrap();

    let err = app.run(InputCommand::Move { source: Path::new("2023").to_owned(), destination: Path::new("2024").to_owned(), force: false }).err().unwrap();
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    if let AppError::Command(CommandError::NoteExistsAtDestination(err_path)) = err {
        app.clear_cache();

        assert_eq!(note3_path, err_path);
        assert_eq!(note_id1, app.note_metadata_storage().unwrap().get_id(note1_path).unwrap());
        assert_eq!(note_id2, app.note_metadata_storage().unwrap().get_id(note2_path).unwrap());
        assert_eq!(note_id3, app.note_metadata_storage().unwrap().get_id(note3_path).unwrap());

        assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path).unwrap());
        assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path).unwrap());
        assert_eq!(note3_content, app.note_metadata_storage().unwrap().get_content(note3_path).unwrap());
    } else {
        assert!(false, "Expected 'NoteAtDestination' error");
    }
}

#[test]
fn test_move_file_to_dir() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_path2 = Path::new("2023/07/01/sample");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: Path::new("2023/07/01/sample2").to_owned(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: note_path.to_owned(), destination: Path::new("2023/07/01").to_owned(), force: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_move_glob1() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note1_path = Path::new("2023/07/sample1");
    let note1_path2 = Path::new("2025/07/sample1");
    let note1_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let note2_path = Path::new("2024/07/sample2");
    let note2_path2 = Path::new("2025/07/sample2");
    let note2_content = r#"Hello, My World!

``` python
import numpy as np
print(np.square(np.arange(0, 15)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note1_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note1_content.clone()
        },
        Command::AddNoteWithContent {
            path: note2_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note2_content.clone()
        }
    ]).unwrap();
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: Path::new("202*").to_path_buf(), destination: Path::new("2025").to_path_buf(), force: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note1_path).is_ok());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note2_path).is_ok());
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path2).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_remove() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Remove { path: note_path.to_owned(), recursive: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get(note_path).is_some());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_remove_recursive() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note1_path = Path::new("2023/07/test1");
    let note1_content = "Test1".to_owned();

    let note2_path = Path::new("2023/07/test2");
    let note2_content = "Test2".to_owned();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note1_path.to_path_buf(),
            tags: vec![],
            content: note1_content.clone()
        },
        Command::AddNoteWithContent {
            path: note2_path.to_path_buf(),
            tags: vec![],
            content: note2_content.clone()
        }
    ]).unwrap();
    assert_eq!(note1_content, app.note_metadata_storage().unwrap().get_content(note1_path).unwrap());
    assert_eq!(note2_content, app.note_metadata_storage().unwrap().get_content(note2_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Remove { path: Path::new("2023").to_owned(), recursive: true }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get(note1_path).is_some());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note1_path).is_ok());
    assert_eq!(false, app.note_metadata_storage().unwrap().get(note2_path).is_some());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note2_path).is_ok());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_change_tags() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.create_and_execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec!["snippet".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(vec!["python".to_owned(), "snippet".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_edit() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();
    let note_content2 = r#"Hello, World!

``` python
xs = list(range(0, 15))
print([x * x for x in xs])
```
"#.to_string();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec![],
            content: note_content.clone()
        },
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    assert_eq!(vec!["snippet".to_owned(), "python".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);

    app.create_and_execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec![],
            content: note_content2.clone()
        },
    ]).unwrap();
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_edit_with_editor() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let mut config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    config.allow_stdin = false;
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();
    let note_content2 = r#"Hello, World!

``` python
xs = list(range(0, 15))
print([x * x for x in xs])
```
"#.to_string();

    let note_content2_clone = note_content2.clone();
    let mut app = App::with_custom(config, move |config, repository| {
        CommandInterpreter::with_launch_editor(
            config,
            repository,
            Box::new(move |_, path| {
                std::fs::write(path, &note_content2_clone).map_err(|err| CommandError::IO(err))
            })
        )
    }).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec![],
            content: note_content.clone()
        },
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    assert_eq!(vec!["snippet".to_owned(), "python".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);

    app.run(InputCommand::Edit {
        path: note_path.to_owned(),
        history: None,
        clear_tags: false,
        add_tags: vec![],
    }).unwrap();
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_edit_with_editor_and_history() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let mut config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    config.allow_stdin = false;
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();
    let note_content2 = r#"Hello, World!

``` python
xs = list(range(0, 15))
print([x * x for x in xs])
```
"#.to_string();

    let mut app = App::with_custom(config, move |config, repository| {
        CommandInterpreter::with_launch_editor(
            config,
            repository,
            Box::new(move |_, _| {
                Ok(())
            })
        )
    }).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec![],
            content: note_content.clone()
        },
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    assert_eq!(vec!["snippet".to_owned(), "python".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);

    app.create_and_execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec![],
            content: note_content2.clone()
        },
    ]).unwrap();
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Edit {
        path: note_path.to_owned(),
        history: Some("HEAD~1".to_owned()),
        clear_tags: false,
        add_tags: vec![],
    }).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(3, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_undo() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample");
    let note_content1 = "Test1".to_owned();
    let note_content2 = "Test2".to_owned();

    let mut app = App::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec![],
            content: note_content1.clone()
        },
    ]).unwrap();
    assert_eq!(note_content1, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.create_and_execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec![],
            content: note_content2.clone()
        },
    ]).unwrap();
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
    let commit_id = repository.reflog("HEAD").unwrap().get(0).unwrap().id_new();

    app.run(InputCommand::Undo { commit: commit_id.to_string() }).unwrap();
    assert_eq!(note_content1, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(3, repository.reflog("HEAD").unwrap().len());
}