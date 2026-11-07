#![windows_subsystem = "windows"]
mod actions;
mod paint;
mod window;

fn main() {
    // The window takes no options, so anything else is refused.
    if std::env::args().len() > 1 {
        std::process::exit(2);
    }
    std::process::exit(window::run());
}
