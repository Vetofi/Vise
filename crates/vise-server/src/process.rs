use std::{ffi::OsStr, process::Command};

pub fn command(program: impl AsRef<OsStr>) -> Command {
    let command = Command::new(program);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::{Console::GetConsoleWindow, Threading::CREATE_NO_WINDOW};
        let mut command = command;
        if unsafe { GetConsoleWindow() }.is_null() {
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    };
    command
}
