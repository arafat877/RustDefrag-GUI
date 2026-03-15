# Contributing to RustDefrag GUI

Thank you for contributing.

## Development Setup

1. Install stable Rust with MSVC target.
2. Build locally:
   ```powershell
   cargo build --release
   ```
3. Run the app:
   ```powershell
   .\target\release\defrag-gui.exe
   ```

## Pull Request Guidelines

- Keep PRs focused and small when possible.
- Include a clear summary of behavioral changes.
- Add screenshots or short recordings for UI changes.
- Ensure `cargo build --release` succeeds before opening PR.

## Code Style

- Follow idiomatic Rust style.
- Keep unsafe logic isolated to `src/defrag_engine/winapi.rs`.
- Prefer small, testable functions.

## Reporting Bugs

Open a GitHub issue and include:

- Windows version
- Rust version
- Reproduction steps
- Expected vs actual behavior
- Logs/screenshots if available
