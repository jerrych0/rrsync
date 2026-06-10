# rrsync

`rrsync` is a Rust-based command-line tool designed to create efficient, incremental backups leveraging the power of `rsync` and hard links. It aims to provide a robust, policy-driven backup solution, combining the battle-tested efficiency of `rsync` with the safety and modern capabilities of a Rust application.

## Features

*   **Multiple Backup Jobs**: Configure and run multiple backup jobs from a single configuration file.
*   **Incremental Backups**: Utilizes `rsync`'s `--link-dest` feature to create new backups that are hard-linked to the most recent previous backup, saving significant disk space for unchanged files.
*   **Timestamped Snapshots**: Each backup is stored in a clearly named, timestamped directory, making it easy to browse and restore specific versions.
*   **Retention Policies**: Automatically prunes old backups based on a flexible, tiered retention policy (keep N daily, weekly, monthly, yearly backups).
*   **Exclusion Patterns**: Exclude specific files or directories from your backups for each job.
*   **Dry-Run Mode**: Preview what changes `rrsync` will make without actually executing any backup or deletion operations.

## How it Works

`rrsync` acts as an intelligent orchestrator for the system's native `rsync` binary. When `rrsync` runs, it:
1.  Parses a `config.toml` file to load one or more backup jobs.
2.  For each job:
    a. Identifies the most recent successful backup in the destination.
    b. Constructs an `rsync` command, using `--link-dest` to point to the previous backup and applying any exclusion rules.
    c. Executes the `rsync` command to create a new, timestamped backup.
    d. Applies the configured retention policy to prune old backups in the destination directory.

## Usage

### Prerequisites

*   Rust toolchain (stable)
*   `rsync` installed and available in your system's PATH.

### 1. Create a Configuration File

Create a `config.toml` file to define your backup jobs.

**Example `config.toml`:**
```toml
# First backup job: Backup documents with a detailed retention policy
[[jobs]]
name = "Documents Backup"
source = "/Users/your_username/Documents"
destination = "/Volumes/MyUSB/Backups/Documents"
exclude = ["*.tmp", "node_modules/"]

[jobs.retention_policy]
keep_daily = 7
keep_weekly = 4
keep_monthly = 6
keep_yearly = 3

# Second backup job: Backup photos with default retention
[[jobs]]
name = "Photos Backup"
source = "/Users/your_username/Pictures"
destination = "/Volumes/MyUSB/Backups/Photos"
# This job will use the default retention policy:
# daily=7, weekly=4, monthly=12, yearly=5

# Third backup job: Backup projects with a minimal retention policy
[[jobs]]
name = "Projects Backup"
source = "/Users/your_username/Developer/Projects"
destination = "/Volumes/MyUSB/Backups/Projects"
exclude = ["target/", "build/"]

[jobs.retention_policy]
keep_daily = 3
keep_weekly = 1
keep_monthly = 1
keep_yearly = 0
```

### 2. Build and Run

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/jerrych0/rrsync.git
    cd rrsync
    ```
2.  **Build the project:**
    ```bash
    cargo build --release
    ```
3.  **Run a backup:**

    Use the `-c` or `--config` flag to point to your configuration file.
    ```bash
    ./target/release/rrsync --config /path/to/your/config.toml
    ```

### 3. Perform a Dry Run (Recommended)

Before running a real backup, use the `--dry-run` flag to preview the actions that `rrsync` will take. This will print the `rsync` commands and the list of backups that would be deleted, without making any changes.

```bash
./target/release/rrsync --config /path/to/your/config.toml --dry-run
```

## Testing

To run the unit tests (which do not interact with the actual filesystem):
```bash
cargo test
```

## Development Phases (Roadmap)

*   **Phase 1: Core Functionality** (Completed)
    *   CLI argument parsing and single-job configuration.
    *   `rsync` command orchestration with `--link-dest`.
*   **Phase 2: Multi-Job & Retention Policy** (Completed)
    *   Support for multiple backup jobs via `config.toml`.
    *   Implemented tiered retention policy (daily, weekly, monthly, yearly).
    *   Added `--dry-run` mode for safe execution previews.
*   **Phase 3: Smart Space Management** (Next)
    *   Add an optional feature to intelligently delete backups when disk space is low.
*   **Phase 4: Pure Rust Transition**
    *   Investigate replacing the external `rsync` dependency with a pure-Rust implementation.

## Contributing

Contributions are welcome! Please feel free to open issues or pull requests.

## License

This project is licensed under the MIT License. See the `LICENSE` file for details.
