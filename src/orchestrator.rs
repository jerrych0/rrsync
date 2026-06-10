// src/orchestrator.rs
use anyhow::{Context, Result};
use std::path::PathBuf;
use chrono::{Local, NaiveDateTime, Datelike};
use crate::config;
use std::collections::HashSet;

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
            .with_context(|| format!("Failed to execute: {} {:?}", command, args))?;

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
    excludes: Vec<String>,
    retention_policy: &'a config::RetentionPolicy,
    dry_run: bool,
}

impl<'a, T: CommandExecutor> Orchestrator<'a, T> {
    pub fn new(
        executor: &'a T,
        source: PathBuf,
        destination: PathBuf,
        excludes: Vec<String>,
        retention_policy: &'a config::RetentionPolicy,
        dry_run: bool,
    ) -> Self {
        Self {
            executor,
            source,
            destination,
            excludes,
            retention_policy,
            dry_run,
        }
    }

    /// Finds the path to the most recent backup in the destination directory.
    /// Backups are assumed to be directories named with timestamps (e.g., 2026-06-09-14-30-00).
    pub fn find_latest_backup(&self) -> Result<Option<PathBuf>> {
        if !self.destination.exists() {
            return Ok(None);
        }
        let mut backups: Vec<PathBuf> = std::fs::read_dir(&self.destination)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.is_dir() {
                    if let Some(file_name) = path.file_name() {
                        if let Some(name_str) = file_name.to_str() {
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
            a.file_name().cmp(&b.file_name())
        });

        Ok(backups.last().cloned())
    }

    fn apply_retention_policy(&self) -> Result<()> {
        if !self.destination.exists() {
            println!("Destination directory does not exist, skipping retention policy.");
            return Ok(());
        }

        let all_backups_info = std::fs::read_dir(&self.destination)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.is_dir() {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .and_then(|name_str| {
                            NaiveDateTime::parse_from_str(name_str, "%Y-%m-%d-%H-%M-%S")
                                .ok()
                                .map(|dt| (dt, path.clone()))
                        })
                } else {
                    None
                }
            })
            .collect::<Vec<(NaiveDateTime, PathBuf)>>();

        let mut sorted_backups_newest_first = all_backups_info.clone();
        sorted_backups_newest_first.sort_by_key(|(dt, _)| *dt);
        sorted_backups_newest_first.reverse();

        let mut kept_by_any_rule: HashSet<PathBuf> = HashSet::new();

        // Daily
        if self.retention_policy.keep_daily > 0 {
            let mut latest_per_day = std::collections::HashMap::new();
            for (dt, path) in sorted_backups_newest_first.iter() {
                latest_per_day.entry(dt.date()).or_insert_with(|| path.clone());
            }
            let mut daily_keepers: Vec<_> = latest_per_day.values().cloned().collect();
            daily_keepers.sort_by(|a, b| b.cmp(a));
            for path in daily_keepers.into_iter().take(self.retention_policy.keep_daily as usize) {
                kept_by_any_rule.insert(path);
            }
        }

        // Weekly
        if self.retention_policy.keep_weekly > 0 {
            let mut latest_per_week = std::collections::HashMap::new();
            for (dt, path) in sorted_backups_newest_first.iter() {
                if !kept_by_any_rule.contains(path) {
                    let week = dt.iso_week();
                    latest_per_week.entry((week.year(), week.week())).or_insert_with(|| path.clone());
                }
            }
            let mut weekly_keepers: Vec<_> = latest_per_week.values().cloned().collect();
            weekly_keepers.sort_by(|a, b| b.cmp(a));
            for path in weekly_keepers.into_iter().take(self.retention_policy.keep_weekly as usize) {
                kept_by_any_rule.insert(path);
            }
        }

        // Monthly
        if self.retention_policy.keep_monthly > 0 {
            let mut latest_per_month = std::collections::HashMap::new();
            for (dt, path) in sorted_backups_newest_first.iter() {
                if !kept_by_any_rule.contains(path) {
                    latest_per_month.entry((dt.year(), dt.month())).or_insert_with(|| path.clone());
                }
            }
            let mut monthly_keepers: Vec<_> = latest_per_month.values().cloned().collect();
            monthly_keepers.sort_by(|a, b| b.cmp(a));
            for path in monthly_keepers.into_iter().take(self.retention_policy.keep_monthly as usize) {
                kept_by_any_rule.insert(path);
            }
        }

        // Yearly
        if self.retention_policy.keep_yearly > 0 {
            let mut latest_per_year = std::collections::HashMap::new();
            for (dt, path) in sorted_backups_newest_first.iter() {
                if !kept_by_any_rule.contains(path) {
                    latest_per_year.entry(dt.year()).or_insert_with(|| path.clone());
                }
            }
            let mut yearly_keepers: Vec<_> = latest_per_year.values().cloned().collect();
            yearly_keepers.sort_by(|a, b| b.cmp(a));
            for path in yearly_keepers.into_iter().take(self.retention_policy.keep_yearly as usize) {
                kept_by_any_rule.insert(path);
            }
        }

        let backups_to_delete: Vec<PathBuf> = all_backups_info
            .into_iter()
            .map(|(_, path)| path)
            .filter(|path| !kept_by_any_rule.contains(path))
            .collect();

        if self.dry_run {
            println!("[DRY RUN] Would apply retention policy. Backups to keep: {}, Backups to delete: {}", kept_by_any_rule.len(), backups_to_delete.len());
            for backup_path in backups_to_delete {
                println!("[DRY RUN] Would delete old backup: {:?}", backup_path);
            }
        } else {
            println!("Applying retention policy. Backups to keep: {}, Backups to delete: {}", kept_by_any_rule.len(), backups_to_delete.len());

            for backup_path in backups_to_delete {
                println!("Deleting old backup: {:?}", backup_path);
                std::fs::remove_dir_all(&backup_path)
                    .with_context(|| format!("Failed to delete old backup: {:?}", backup_path))?;
            }
        }

        Ok(())
    }

    pub fn run_backup(&self) -> Result<()> {
        let current_backup_name = Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();
        let current_backup_path = self.destination.join(&current_backup_name);

        if !self.dry_run {
            std::fs::create_dir_all(&self.destination)
                .with_context(|| format!("Failed to create destination directory: {:?}", self.destination))?;
        }

        let mut rsync_args_owned: Vec<String> = vec![
            "-av".to_string(),
            "--delete".to_string(),
        ];

        if self.dry_run {
            rsync_args_owned.push("--dry-run".to_string());
            println!("[DRY RUN] Destination path would be: {:?}", current_backup_path);
        }

        for exclude_pattern in &self.excludes {
            rsync_args_owned.push(format!("--exclude={}", exclude_pattern));
        }

        if let Some(latest_backup) = self.find_latest_backup()? {
            let link_dest_arg = format!("--link-dest={}", latest_backup.display());
            rsync_args_owned.push(link_dest_arg);
        }

        rsync_args_owned.push(self.source.to_str().context("Invalid source path")?.to_string());
        rsync_args_owned.push(current_backup_path.to_str().context("Invalid current backup path")?.to_string());

        println!("Executing rsync command: rsync {}", rsync_args_owned.join(" "));

        if !self.dry_run {
            let rsync_args_refs: Vec<&str> = rsync_args_owned.iter().map(|s| s.as_str()).collect();
            self.executor.execute("rsync", &rsync_args_refs)?;
            println!("Backup completed successfully to {:?}", current_backup_path);
        }

        self.apply_retention_policy()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;
    use std::fs;
    use std::path::Path;
    use chrono::{Duration, NaiveDate, Datelike, Weekday};
    use std::collections::HashSet;

    fn create_dummy_backup(parent_dir: &Path, name: &str) -> PathBuf {
        let path = parent_dir.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_test_orchestrator<'a>(
        mock_executor: &'a MockCommandExecutor,
        destination: PathBuf,
        retention_policy: &'a config::RetentionPolicy,
        dry_run: bool,
    ) -> Orchestrator<'a, MockCommandExecutor> {
        Orchestrator::new(
            mock_executor,
            PathBuf::from("/tmp/source"),
            destination,
            vec![],
            retention_policy,
            dry_run,
        )
    }

    #[test]
    fn test_find_latest_backup_no_backups() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy::default();
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let latest = orchestrator.find_latest_backup().unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn test_find_latest_backup_with_backups() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _b1 = create_dummy_backup(temp_dir.path(), "2023-01-01-10-00-00");
        let b2 = create_dummy_backup(temp_dir.path(), "2023-01-01-11-00-00");
        let _b3 = create_dummy_backup(temp_dir.path(), "2023-01-01-09-00-00");

        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy::default();
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let latest = orchestrator.find_latest_backup().unwrap().unwrap();
        assert_eq!(latest.file_name().unwrap().to_str().unwrap(), "2023-01-01-11-00-00");
        assert_eq!(latest, b2);
    }
    
    #[test]
    fn test_find_latest_backup_ignores_invalid_names() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _invalid_dir = create_dummy_backup(temp_dir.path(), "not-a-timestamp");
        let valid_dir = create_dummy_backup(temp_dir.path(), "2024-01-01-12-00-00");

        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy::default();
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let latest = orchestrator.find_latest_backup().unwrap().unwrap();
        assert_eq!(latest.file_name().unwrap().to_str().unwrap(), "2024-01-01-12-00-00");
        assert_eq!(latest, valid_dir);
    }

    #[test]
    fn test_run_backup_first_backup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = PathBuf::from("/tmp/source");
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy::default();

        let orchestrator = Orchestrator::new(
            &mock_executor,
            source_path.clone(),
            temp_dir.path().to_path_buf(),
            vec![],
            &retention_policy,
            false,
        );

        orchestrator.run_backup().unwrap();

        let commands = mock_executor.commands.borrow();
        assert_eq!(commands.len(), 1);
        let (cmd, args) = &commands[0];
        assert_eq!(cmd, "rsync");
        assert!(args.contains(&source_path.to_str().unwrap().to_string()));
        assert!(args.iter().any(|arg| arg.starts_with(&temp_dir.path().to_str().unwrap().to_string())));
        assert!(!args.iter().any(|arg| arg.starts_with("--link-dest=")));
        assert!(!args.iter().any(|arg| arg.starts_with("--exclude=")));
    }

    #[test]
    fn test_run_backup_incremental_backup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = PathBuf::from("/tmp/source");
        
        let previous_backup_name = "2023-10-26-10-00-00";
        create_dummy_backup(temp_dir.path(), previous_backup_name);

        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy::default();

        let orchestrator = Orchestrator::new(
            &mock_executor,
            source_path.clone(),
            temp_dir.path().to_path_buf(),
            vec![],
            &retention_policy,
            false,
        );

        orchestrator.run_backup().unwrap();

        let commands = mock_executor.commands.borrow();
        assert_eq!(commands.len(), 1);
        let (cmd, args) = &commands[0];
        assert_eq!(cmd, "rsync");
        assert!(args.contains(&source_path.to_str().unwrap().to_string()));
        assert!(args.iter().any(|arg| arg.starts_with("--link-dest=")));
        assert!(args.iter().any(|arg| arg.contains(previous_backup_name)));
        assert!(!args.iter().any(|arg| arg.starts_with("--exclude=")));
    }

    #[test]
    fn test_run_backup_with_excludes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = PathBuf::from("/tmp/source");
        let mock_executor = MockCommandExecutor::new();
        let excludes = vec!["target".to_string(), "*.log".to_string()];
        let retention_policy = config::RetentionPolicy::default();

        let orchestrator = Orchestrator::new(
            &mock_executor,
            source_path.clone(),
            temp_dir.path().to_path_buf(),
            excludes.clone(),
            &retention_policy,
            false,
        );

        orchestrator.run_backup().unwrap();

        let commands = mock_executor.commands.borrow();
        assert_eq!(commands.len(), 1);
        let (cmd, args) = &commands[0];
        assert_eq!(cmd, "rsync");
        assert!(args.contains(&source_path.to_str().unwrap().to_string()));
        assert!(args.iter().any(|arg| arg == "--exclude=target"));
        assert!(args.iter().any(|arg| arg == "--exclude=*.log"));
    }

    // --- Retention Policy Tests ---

    #[test]
    fn test_apply_retention_policy_daily() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy {
            keep_daily: 2,
            keep_weekly: 0,
            keep_monthly: 0,
            keep_yearly: 0,
        };
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let today = Local::now().naive_local().date();
        let yesterday = today - Duration::days(1);
        let two_days_ago = today - Duration::days(2);
        let three_days_ago = today - Duration::days(3);

        create_dummy_backup(temp_dir.path(), &today.format("%Y-%m-%d-10-00-00").to_string());
        create_dummy_backup(temp_dir.path(), &today.format("%Y-%m-%d-11-00-00").to_string()); // Latest today
        create_dummy_backup(temp_dir.path(), &yesterday.format("%Y-%m-%d-10-00-00").to_string());
        create_dummy_backup(temp_dir.path(), &yesterday.format("%Y-%m-%d-11-00-00").to_string()); // Latest yesterday
        create_dummy_backup(temp_dir.path(), &two_days_ago.format("%Y-%m-%d-10-00-00").to_string());
        create_dummy_backup(temp_dir.path(), &two_days_ago.format("%Y-%m-%d-11-00-00").to_string()); // Latest 2 days ago
        create_dummy_backup(temp_dir.path(), &three_days_ago.format("%Y-%m-%d-10-00-00").to_string()); // Latest 3 days ago

        orchestrator.apply_retention_policy().unwrap();

        let remaining_backups_paths: HashSet<String> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_name()?.to_str().map(|s| s.to_string()))
            .collect();
        
        let expected_backups: HashSet<String> = [
            today.format("%Y-%m-%d-11-00-00").to_string(),
            yesterday.format("%Y-%m-%d-11-00-00").to_string(),
        ].iter().cloned().collect();

        assert_eq!(remaining_backups_paths, expected_backups);
    }

    #[test]
    fn test_apply_retention_policy_no_daily_kept() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy {
            keep_daily: 0,
            keep_weekly: 0,
            keep_monthly: 0,
            keep_yearly: 0,
        };
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let today = Local::now().naive_local().date();
        let yesterday = today - Duration::days(1);
        create_dummy_backup(temp_dir.path(), &today.format("%Y-%m-%d-11-00-00").to_string());
        create_dummy_backup(temp_dir.path(), &yesterday.format("%Y-%m-%d-11-00-00").to_string());

        orchestrator.apply_retention_policy().unwrap();

        let remaining_backups_paths: HashSet<String> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_name()?.to_str().map(|s| s.to_string()))
            .collect();
        
        assert!(remaining_backups_paths.is_empty());
    }

    #[test]
    fn test_apply_retention_policy_weekly() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy {
            keep_daily: 0,
            keep_weekly: 2,
            keep_monthly: 0,
            keep_yearly: 0,
        };
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let today = Local::now().naive_local().date();
        let mut backups_to_create = Vec::new();

        // Create 4 weeks of backups, one per week
        for i in 0..4 {
            let date = today - Duration::weeks(i);
            // Construct NaiveDate directly using from_isoywd_opt
            let backup_date = NaiveDate::from_isoywd_opt(date.year(), date.iso_week().week(), Weekday::Mon)
                                .unwrap()
                                .and_hms_opt(12, 0, 0)
                                .unwrap();
            backups_to_create.push(create_dummy_backup(temp_dir.path(), &backup_date.format("%Y-%m-%d-%H-%M-%S").to_string()));
        }

        orchestrator.apply_retention_policy().unwrap();

        let remaining_backups_paths: HashSet<String> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_name()?.to_str().map(|s| s.to_string()))
            .collect();

        // Should keep 2 latest weekly backups
        let expected_backups: HashSet<String> = [
            NaiveDate::from_isoywd_opt(today.year(), today.iso_week().week(), Weekday::Mon).unwrap().and_hms_opt(12, 0, 0).unwrap().format("%Y-%m-%d-%H-%M-%S").to_string(),
            NaiveDate::from_isoywd_opt((today - Duration::weeks(1)).year(), (today - Duration::weeks(1)).iso_week().week(), Weekday::Mon).unwrap().and_hms_opt(12, 0, 0).unwrap().format("%Y-%m-%d-%H-%M-%S").to_string(),
        ].iter().cloned().collect();
        
        assert_eq!(remaining_backups_paths, expected_backups);
    }

    #[test]
    fn test_apply_retention_policy_monthly() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy {
            keep_daily: 0,
            keep_weekly: 0,
            keep_monthly: 2,
            keep_yearly: 0,
        };
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let today = Local::now().naive_local().date();
        let mut backups_to_create = Vec::new();

        // Create 4 months of backups, one per month (e.g., first day of month)
        for i in 0..4 {
            let date = today.with_day(1).unwrap() - Duration::days(30 * i); // Approx month, always first of month
            // Find the 1st day of the month for that month
            let backup_date = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
                                .unwrap()
                                .and_hms_opt(12, 0, 0)
                                .unwrap();
            backups_to_create.push(create_dummy_backup(temp_dir.path(), &backup_date.format("%Y-%m-%d-%H-%M-%S").to_string()));
        }

        orchestrator.apply_retention_policy().unwrap();

        let remaining_backups_paths: HashSet<String> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_name()?.to_str().map(|s| s.to_string()))
            .collect();

        // Should keep 2 latest monthly backups
        let expected_backups: HashSet<String> = [
            NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap().and_hms_opt(12, 0, 0).unwrap().format("%Y-%m-%d-%H-%M-%S").to_string(),
            NaiveDate::from_ymd_opt((today - Duration::days(30)).year(), (today - Duration::days(30)).month(), 1).unwrap().and_hms_opt(12, 0, 0).unwrap().format("%Y-%m-%d-%H-%M-%S").to_string(),
        ].iter().cloned().collect();
        
        assert_eq!(remaining_backups_paths, expected_backups);
    }

    #[test]
    fn test_apply_retention_policy_yearly() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy {
            keep_daily: 0,
            keep_weekly: 0,
            keep_monthly: 0,
            keep_yearly: 2,
        };
        let orchestrator = create_test_orchestrator(&mock_executor, temp_dir.path().to_path_buf(), &retention_policy, false);

        let today = Local::now().naive_local().date();
        let mut backups_to_create = Vec::new();

        // Create 4 years of backups, one per year (e.g., first day of year)
        for i in 0..4 {
            let year = today.year() - i;
            let backup_date = NaiveDate::from_ymd_opt(year, 1, 1)
                                .unwrap()
                                .and_hms_opt(12, 0, 0)
                                .unwrap();
            backups_to_create.push(create_dummy_backup(temp_dir.path(), &backup_date.format("%Y-%m-%d-%H-%M-%S").to_string()));
        }

        orchestrator.apply_retention_policy().unwrap();

        let remaining_backups_paths: HashSet<String> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.path().file_name()?.to_str().map(|s| s.to_string()))
            .collect();

        // Should keep 2 latest yearly backups
        let expected_backups: HashSet<String> = [
            NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap().and_hms_opt(12, 0, 0).unwrap().format("%Y-%m-%d-%H-%M-%S").to_string(),
            NaiveDate::from_ymd_opt(today.year() - 1, 1, 1).unwrap().and_hms_opt(12, 0, 0).unwrap().format("%Y-%m-%d-%H-%M-%S").to_string(),
        ].iter().cloned().collect();
        
        assert_eq!(remaining_backups_paths, expected_backups);
    }

    #[test]
    fn test_dry_run_mode_does_not_execute_or_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = PathBuf::from("/tmp/source");
        let mock_executor = MockCommandExecutor::new();
        let retention_policy = config::RetentionPolicy {
            keep_daily: 0,
            keep_weekly: 0,
            keep_monthly: 0,
            keep_yearly: 0,
        };

        // Create a backup that would be deleted if not in dry-run mode
        create_dummy_backup(temp_dir.path(), "2023-01-01-10-00-00");
        assert_eq!(std::fs::read_dir(temp_dir.path()).unwrap().count(), 1);

        let orchestrator = Orchestrator::new(
            &mock_executor,
            source_path.clone(),
            temp_dir.path().to_path_buf(),
            vec![],
            &retention_policy,
            true, // Enable dry run
        );

        orchestrator.run_backup().unwrap();

        // Assert that no rsync command was executed
        let commands = mock_executor.commands.borrow();
        assert!(commands.is_empty());

        // Assert that the backup directory was not deleted
        assert_eq!(std::fs::read_dir(temp_dir.path()).unwrap().count(), 1);
    }
}
