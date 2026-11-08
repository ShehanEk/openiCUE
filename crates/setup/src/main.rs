#![windows_subsystem = "windows"]
mod install;
mod paint;
mod shortcut;
mod window;

use windows_sys::Win32::System::LibraryLoader::{SetDefaultDllDirectories, LOAD_LIBRARY_SEARCH_SYSTEM32};

fn main() {
    // Setup is started from Downloads, so never load a DLL from beside it.
    // SAFETY: plain call, first thing in the process.
    unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let uninstall = match args.as_slice() {
        [] => false,
        [flag] if flag == "--uninstall" => true,
        _ => std::process::exit(2),
    };
    std::process::exit(window::run(uninstall));
}
