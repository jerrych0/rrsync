# rrsync

`rrsync` is a Rust-based command-line tool designed to create efficient, incremental backups leveraging the power of `rsync` and hard links. It aims to provide a robust, policy-driven backup solution, combining the battle-tested efficiency of `rsync` with the safety and modern capabilities of a Rust application.

## Features (Current)

*   **Incremental Backups**: Utilizes `rsync`'s `--link-dest` feature to create new backups that are hard-linked to the most recent previous backup, saving significant disk space for unchanged files.
*   **Timestamped Snapshots**: Each backup is stored in a clearly named, timestamped directory, making it easy to browse and restore specific versions.
*   **Configurable**: Supports configuration via command-line arguments.

## How it Works

`rrsync` acts as an intelligent orchestrator for the system's native `rsync` binary. When `rrsync` runs, it:
1.  Identifies the most recent successful backup in the destination.
2.  Constructs an `rsync` command, using `--link-dest` to point to the previous backup (if one exists).
3.  Executes the `rsync` command, creating a new, timestamped backup directory that uses hard links for unchanged files.

## Usage

### Prerequisites

*   Rust toolchain (stable)
*   `rsync` installed and available in your system's PATH.

### Build and Run

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/your-username/rrsync.git # (Replace with actual repo URL)
    cd rrsync
    ```
2.  **Build the project:**
    ```bash
    cargo build --release
    ```
3.  **Run a backup:**

    To perform a backup from `/path/to/source` to `/path/to/destination`:
    ```bash
    ./target/release/rrsync -s /path/to/source -d /path/to/destination
    ```
    Replace `/path/to/source` and `/path/to/destination` with your actual directories.

    **Example:**
    ```bash
    mkdir -p /tmp/my_source
    echo "This is file1" > /tmp/my_source/file1.txt
    echo "This is file2" > /tmp/my_source/file2.txt
    mkdir -p /tmp/my_destination

    ./target/release/rrsync -s /tmp/my_source -d /tmp/my_destination
    # Run again to create an incremental backup
    echo "New content for file1" > /tmp/my_source/file1.txt
    echo "This is file3" > /tmp/my_source/file3.txt
    ./target/release/rrsync -s /tmp/my_source -d /tmp/my_destination
    ```

    You can then inspect `/tmp/my_destination` to see the timestamped backup directories.

## Testing

### Unit Tests

To run the unit tests (which do not interact with the actual filesystem):
```bash
cargo test
```

### Integration Tests (Planned)

Integration tests will involve actual filesystem interaction and running the compiled `rrsync` binary. These are planned for future development.

## Development Phases (Roadmap)

*   **Phase 1: Core Functionality** (Completed)
    *   CLI argument parsing and configuration loading.
    *   `rsync` command orchestration with `--link-dest` for incremental backups.
    *   Finding the latest backup in the destination.
*   **Phase 2: Retention Policy** (Next)
    *   Implement logic to automatically prune old backups based on user-defined policies (e.g., keep 7 daily, 4 weekly).
*   **Phase 3: Smart Space Management**
    *   Add an optional feature to intelligently delete backups when disk space is low.
*   **Phase 4: Pure Rust Transition**
    *   Investigate replacing the external `rsync` dependency with a pure-Rust implementation.

## Contributing

Contributions are welcome! Please feel free to open issues or pull requests.

## License

This project is licensed under the MIT License. See the `LICENSE` file for details.
