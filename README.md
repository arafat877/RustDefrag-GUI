# RustDefrag GUI

High-performance NTFS analysis and defragmentation desktop application built in Rust with a custom `egui/eframe` interface.

[![CI](https://github.com/arafat877/rust-defrag/actions/workflows/ci.yml/badge.svg)](https://github.com/arafat877/rust-defrag/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Overview

RustDefrag GUI combines low-level NTFS cluster operations with a modern real-time visualization layer:

- Live cluster map for free, used, fragmented, moving, done, and system regions
- Real-time analysis feedback with animated progress and map updates
- Defragmentation pipeline with background worker execution
- Dashboard panels for volume, analysis report, and defragmentation results
- Built-in charts for distribution and performance trends

## Highlights

- Native Win32 filesystem control integration (`FSCTL_GET_VOLUME_BITMAP`, `FSCTL_GET_RETRIEVAL_POINTERS`, `FSCTL_MOVE_FILE`)
- Worker-thread architecture to keep UI responsive
- Capped parallel analysis pool to prevent full CPU saturation
- Boot-time retry support for locked files (`MoveFileExW`)
- File whitelist support for highly volatile system/update artifacts
- Custom-drawn rich UI (panels, map, charts, counters)

## Requirements

- Windows 10/11 (x64)
- Rust stable (MSVC target recommended)
- Administrator privileges recommended for full raw-volume visibility and defrag operations

## Quick Start

```powershell
git clone https://github.com/arafat877/rust-defrag
cd rust-defrag-gui
cargo build --release
.\target\release\defrag-gui.exe
```

## Project Structure

```text
src/
  main.rs                Application entry point
  app.rs                 Main UI and state orchestration
  engine/
    messages.rs          GUI <-> worker messages
    worker.rs            Background command execution loop
  defrag_engine/
    volume.rs            Volume metadata, bitmap, file enumeration
    analyzer.rs          File fragmentation analysis
    defrag.rs            Defragmentation execution
    winapi.rs            Isolated unsafe Win32 wrappers
    errors.rs            Domain errors
    whitelist.rs         Skip-list rules
  ui/
    cluster_map.rs       Live cluster grid renderer
    stats_panel.rs       Stats cards and report panels
    charts.rs            Charts (pie, bar, histogram, line)
    theme.rs             Colors and visual constants
```

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `F5` | Start analysis |
| `F6` | Start defragmentation |
| `Esc` | Stop current operation |

## Security and Safety Notes

- Defragmentation touches low-level disk structures and should be run carefully.
- Use backups and avoid force-stopping the app during active relocation operations.
- See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## License

MIT License. See [LICENSE](LICENSE).
