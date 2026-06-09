// src/orchestrator.rs
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use chrono::{Local, NaiveDateTime};

// Trait for abstracting command execution for mocking
pub trait CommandExecutor {
    fn execute(&self, command: &str, args: &[&str]) -> Result<String>;
}

// Production implementation of CommandExecutor that runs actual commands
pub struct RealCommandExecutor;

impl CommandExecutor for RealCommandExecutor {
    fn execute(&self, command: &str, args: &[&str]) -> Result<String> {
        let output = std::process::Command::new(command)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute command: {}", command))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!(
                "Command `{}` failed with exit code {:?}
Stdout: {}
Stderr: {}",
                command,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}

// Mock implementation of CommandExecutor for testing
pub struct MockCommandExecutor {
    pub commands: std::cell::RefCell<Vec<(String, Vec<String>)>>,
    pub results: std::cell::RefCell<Vec<Result<String>>>,
}

impl MockCommandExecutor {
    pub fn new() -> Self {
        Self {
            commands: std::cell::RefCell::new(Vec::new()),
            results: std::cell::RefCell::new(Vec::new()),
        }
    }

    pub fn with_results(results: Vec<Result<String>>) -> Self {
        Self {
            commands: std::cell::RefCell::new(Vec::new()),
            results: std::cell::RefCell::new(results),
        }
    }
}

impl CommandExecutor for MockCommandExecutor {
    fn execute(&self, command: &str, args: &[&str]) -> Result<String> {
        self.commands.borrow_mut().push((command.to_string(), args.iter().map(|&s| s.to_string()).collect()));

        let mut results_borrow = self.results.borrow_mut();
        if results_borrow.is_empty() {
            Ok("".to_string()) // Default successful empty output
        } else {
            results_borrow.remove(0) // Pop the first result
        }
    }
}

pub struct Orchestrator<'a, T: CommandExecutor> {
    executor: &'a T,
    source: PathBuf,
    destination: PathBuf,
}

impl<'a, T: CommandExecutor> Orchestrator<'a, T> {
    pub fn new(executor: &'a T, source: PathBuf, destination: PathBuf) -> Self {
        Self {
            executor,
            source,
            destination,
        }
    }

    /// Finds the path to the most recent backup in the destination directory.
    /// Backups are assumed to be directories named with timestamps (e.g., 2026-06-09-14-30-00).
    pub fn find_latest_backup(&self) -> Result<Option<PathBuf>> {
        let mut backups: Vec<PathBuf> = std::fs::read_dir(&self.destination)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.is_dir() {
                    // Attempt to parse the directory name as a timestamp
                    if let Some(file_name) = path.file_name() {
                        if let Some(name_str) = file_name.to_str() {
                            // Assuming format like "YYYY-MM-DD-HH-MM-SS"
                            if NaiveDateTime::parse_from_str(name_str, "%Y-%m-%d-%H-%M-%S").is_ok() {
                                return Some(path);
                            }
                        }
                    }
                }
                None
            })
            .collect();

        backups.sort_by(|a, b| {
            // Compare by file name (timestamp string)
            a.file_name().cmp(&b.file_name())
        });

        Ok(backups.last().cloned())
    }

    pub fn run_backup(&self) -> Result<()> {
        let current_backup_name = Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();
        let current_backup_path = self.destination.join(&current_backup_name);

        // Ensure the destination directory exists
        std::fs::create_dir_all(&self.destination)
            .with_context(|| format!("Failed to create destination directory: {:?}", self.destination))?;

        // Change rsync_args to Vec<String> to own the string data
        let mut rsync_args_owned: Vec<String> = vec![
            "-av".to_string(), // Archive mode, verbose
            "--delete".to_string(), // Delete extraneous files from dest dirs
            "--exclude=.DS_Store".to_string(), // Example exclusion
        ];

        if let Some(latest_backup) = self.find_latest_backup()? {
            let link_dest_arg = format!("--link-dest={}", latest_backup.display());
            rsync_args_owned.push(link_dest_arg); // Push the owned String
        }

        rsync_args_owned.push(self.source.to_str().context("Invalid source path")?.to_string());
        rsync_args_owned.push(current_backup_path.to_str().context("Invalid current backup path")?.to_string());

        println!("Executing rsync command: rsync {}", rsync_args_owned.join(" "));

        // Convert Vec<String> to Vec<&str> for the executor
        let rsync_args_refs: Vec<&str> = rsync_args_owned.iter().map(|s| s.as_str()).collect();

        self.executor.execute("rsync", &rsync_args_refs)?;

        println!("Backup completed successfully to {:?}", current_backup_path);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    // Helper function to create dummy backup directories
    fn create_dummy_backup(parent_dir: &Path, name: &str) -> PathBuf {
        let path = parent_dir.join(name);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn test_find_latest_backup_no_backups() {
        let temp_dir = tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let orchestrator = Orchestrator::new(
            &mock_executor,
            PathBuf::from("/tmp/source"),
            temp_dir.path().to_path_buf(),
        );

        let latest = orchestrator.find_latest_backup().unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn test_find_latest_backup_with_backups() {
        let temp_dir = tempdir().unwrap();
        let _b1 = create_dummy_backup(temp_dir.path(), "2023-01-01-10-00-00");
        let b2 = create_dummy_backup(temp_dir.path(), "2023-01-01-11-00-00");
        let _b3 = create_dummy_backup(temp_dir.path(), "2023-01-01-09-00-00"); // Older, out of order

        let mock_executor = MockCommandExecutor::new();
        let orchestrator = Orchestrator::new(
            &mock_executor,
            PathBuf::from("/tmp/source"),
            temp_dir.path().to_path_buf(),
        );

        let latest = orchestrator.find_latest_backup().unwrap().unwrap();
        assert_eq!(latest.file_name().unwrap().to_str().unwrap(), "2023-01-01-11-00-00");
        assert_eq!(latest, b2);
    }
    
    // Test that find_latest_backup handles invalid directory names gracefully
    #[test]
    fn test_find_latest_backup_ignores_invalid_names() {
        let temp_dir = tempdir().unwrap();
        let _invalid_dir = create_dummy_backup(temp_dir.path(), "not-a-timestamp");
        let valid_dir = create_dummy_backup(temp_dir.path(), "2024-01-01-12-00-00");

        let mock_executor = MockCommandExecutor::new();
        let orchestrator = Orchestrator::new(
            &mock_executor,
            PathBuf::from("/tmp/source"),
            temp_dir.path().to_path_buf(),
        );

        let latest = orchestrator.find_latest_backup().unwrap().unwrap();
        assert_eq!(latest.file_name().unwrap().to_str().unwrap(), "2024-01-01-12-00-00");
        assert_eq!(latest, valid_dir);
    }

    #[test]
    fn test_run_backup_first_backup() {
        let temp_dir = tempdir().unwrap(); // Use tempdir for destination
        let source_path = PathBuf::from("/path/to/source");
        let mock_executor = MockCommandExecutor::new();

        let orchestrator = Orchestrator::new(
            &mock_executor,
            source_path.clone(),
            temp_dir.path().to_path_buf(), // Use temp_dir for destination
        );

        orchestrator.run_backup().unwrap();

        let commands = mock_executor.commands.borrow();
        assert_eq!(commands.len(), 1);
        let (cmd, args) = &commands[0];
        assert_eq!(cmd, "rsync");
        assert!(args.contains(&source_path.to_str().unwrap().to_string()));
        assert!(args.iter().any(|arg| arg.starts_with(&temp_dir.path().to_str().unwrap().to_string())));
        assert!(!args.iter().any(|arg| arg.starts_with("--link-dest="))); // No link-dest for first backup
    }

    #[test]
    fn test_run_backup_incremental_backup() {
        let temp_dir = tempdir().unwrap();
        let source_path = PathBuf::from("/path/to/source");
        
        // Simulate a previous backup
        let previous_backup_name = "2023-10-26-10-00-00";
        create_dummy_backup(temp_dir.path(), previous_backup_name);

        let mock_executor = MockCommandExecutor::new();

        let orchestrator = Orchestrator::new(
            &mock_executor,
            source_path.clone(),
            temp_dir.path().to_path_buf(),
        );

        orchestrator.run_backup().unwrap();

        let commands = mock_executor.commands.borrow();
        assert_eq!(commands.len(), 1);
        let (cmd, args) = &commands[0];
        assert_eq!(cmd, "rsync");
        assert!(args.contains(&source_path.to_str().unwrap().to_string()));
        assert!(args.iter().any(|arg| arg.starts_with("--link-dest=")));
        assert!(args.iter().any(|arg| arg.contains(previous_backup_name)));
    }
}
