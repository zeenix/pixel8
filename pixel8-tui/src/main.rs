//! Pixel8 in the terminal: the same console the `pixel8` binary opens in a
//! window — shell, editors, running carts — rendered with sixel graphics
//! where the terminal supports them and unicode half-blocks otherwise.
//!
//! This binary is just argument parsing; `tui.rs` is the frontend and the
//! `pixel8-console` library is everything behind it.

mod raw_keys;
mod tui;

use anyhow::{bail, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cli(&strs)
}

fn run_cli(args: &[&str]) -> Result<()> {
    match args {
        ["help" | "--help" | "-h"] => {
            print_help();
            Ok(())
        }
        [] => tui::run(None, false),
        ["run", path] => tui::run(Some(path.to_string()), true),
        ["run"] => {
            print_help();
            bail!("Usage: pixel8-tui run <dir|cart.png>");
        }
        [path] if !path.starts_with('-') => tui::run(Some(path.to_string()), false),
        _ => {
            print_help();
            bail!("Unrecognized arguments: {args:?}");
        }
    }
}

fn print_help() {
    println!(
        "Pixel8 TUI {} - the Pixel8 console in your terminal\n\n\
         Usage:\n\
         \x20 pixel8-tui                  Boot the console\n\
         \x20 pixel8-tui <dir|cart.png>   Boot with a cart loaded\n\
         \x20 pixel8-tui run <dir|cart.png>\n\
         \x20                            Boot, load, and run immediately\n\n\
         Draws with sixel where the terminal supports it, unicode\n\
         half-blocks otherwise. Ctrl+Q quits.",
        pixel8_console::shell::VERSION
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_requires_a_path() {
        let err = run_cli(&["run"]).unwrap_err();
        assert!(err.to_string().contains("Usage"), "got: {err}");
    }

    #[test]
    fn extra_arguments_are_rejected() {
        let err = run_cli(&["a", "b"]).unwrap_err();
        assert!(err.to_string().contains("Unrecognized"), "got: {err}");
    }

    #[test]
    fn flags_are_not_paths() {
        let err = run_cli(&["--bogus"]).unwrap_err();
        assert!(err.to_string().contains("Unrecognized"), "got: {err}");
    }
}
