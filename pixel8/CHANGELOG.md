# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.0 - 2026-09-27

### ✨ Added
- Let the plume effects and a gusting wind be constants.
- Let the World draw its cast.
- Let the console draw a whole cast in one call.
- Size the step by the cast it is handed.
- Step the whole cast through one World, on the console's side.
- Hold a Kinetic inside the edge of the world.
- Give a Kinetic one rectangle, and judge everything against it.
- Add a read-only body accessor to Kinetic.
- Add an explosion to the plume effects.
- Let a wind stand in for a plume's sway.
- Add a physics module: gravity, wind, air and mass for cart entities.
- Let a plume stop at the source and drift out.
- Let a small plume be thinned out to a wisp.
- Let a plume turn, and default it to rising.
- Let a plume move, so it can trail something.
- Add fire and smoke plume effects to the SDK.

### 💥 Breaking
- Keep a MemberId, and borrow the member from the world.
- Make and ask a member through Member.
- Ship a cart's opening state as data, and boot into it.
- Give the World the cast to keep.

### ♻️ Changed
- Add project icon to the SDK docs.
- Add icon to the README.
- Remove a redundant empty line.
- Rename the physics map collider to Collider.
- Split the plume module into a file per effect.

### 📝 Documentation
- Give the real reason the atmosphere writes its clamp out.
- Describe the physics world as it is, not as it came to be.
- Call the built-in font 4x7, the cell it is drawn in.
- Tell xterm users to start it as a VT340 for sixels.
- Count the ABI's fifty imports in the README.
- Keep the window in a silent console build.
- Say what the per-frame work budget actually counts.
- Populate the SDK crate docs from the README.
- Point the README, agents guide and SDK docs at the tutorial book.

### 🐛 Fixed
- Tell both parties of a meeting, whichever one moved.
- Keep floor_i16 from underflowing off the coordinate space.
- Stop the map resolution misplacing a hitbox at either extreme.

### ⚡️ Performance
- Tell a live member id from a stale one with one compare.
- Inline every method of a borrowed member.
- Keep the stale-handle check off every accessor's hot path.
- Keep the wire's records on the stack, for the length of one call.

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
