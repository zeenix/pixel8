# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - 2026-07-13

### Added
- ✨ Give carts a persistent key-value save store.
- ✨ Make ABI positions and sizes f32 for sub-pixel drawing.

### Changed
- 🔧 Publish the README on each crate's crates.io page.
- 🚚 Rename the project from RICO-8 to Pixel8.

### Documentation
- 📝 Document the terminal frontend.
- 📝 Document the JSON asset, cart, and clipboard formats.
- 📝 Remove the "Status" section.
- 📝 Stop implying carts depend on heapless by default.
- 📝 Recast the README as the crates.io landing page.
- 📝 Reflow the README prose to 100 columns.
- 📝 Hide an internal pub mode from docs.
- 📝 Move the PICO-8 comparison out of the README description.
- 📝 Document the windowed desktop player and picker quit.
- 📝 Document the static-musl KMS handheld player.
- 📝 Document the renamed flags-typed SDK API.
- 📝 document PICO-8 asset import.
- 📝 no_std is the default cart path.
- 📝 document the 128K cart limits and the no_std path.

### Fixed
- 🐛 Fix the non-compiling example in the README.

### Other
- Drop redundant Rico8 prefix from the game trait.
- Run carts at 60 fps by default, selectable down to 30.
- Add rico8-player: SDL2 cart player for aarch64 handhelds.
- Visible save feedback and a background check build.
- Add web export (stage 10): single-file playable HTML pages.
- Split the console back into its own crate: rico8-console.
- Add Fedora ALSA package name.
- Implement RICO-8: a PICO-8-like fantasy console for Rust games.
