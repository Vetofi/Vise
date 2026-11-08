use vise_world::datafix::files;
use std::{fs, io, path::Path, process::ExitCode};

fn copy(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &target)?;
        } else if entry.file_name() != "session.lock" {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn main() -> ExitCode {
    let args: Vec<String> = crate::args().skip(1).collect();
    let (input, output, target) = match args.as_slice() {
        [input, output] => (input, output, None),
        [input, output, target] => (input, output, target.parse::<i32>().ok()),
        _ => {
            eprintln!("usage: datafix_world IN_WORLD OUT_WORLD [TARGET]");
            return ExitCode::from(2);
        }
    };
    let output = Path::new(output);
    let result = (|| {
        if output.exists() {
            fs::remove_dir_all(output)?;
        }
        copy(Path::new(input), output)?;
        match target {
            Some(target) => files::upgrade_band(output, target).map(|()| target),
            None => files::upgrade_world(output),
        }
    })();
    match result {
        Ok(version) => {
            println!("upgraded {} from DataVersion {version}", output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("datafix_world: {e}");
            ExitCode::FAILURE
        }
    }
}
