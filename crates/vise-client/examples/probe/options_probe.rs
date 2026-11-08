#![forbid(unsafe_code)]
use vise_client::{
    Result, keybinds,
    options_file::{OptionsFile, V},
};
use std::{fs, io::Write};

pub fn main() -> Result<()> {
    let args: Vec<_> = crate::args().skip(1).collect();
    let mut names: Vec<_> = fs::read_dir(&args[0])?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    names.sort();
    let mut out = fs::File::create(&args[1])?;
    let mut first = true;
    for path in names {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut file = if name == "absent.txt" {
            OptionsFile::default()
        } else {
            OptionsFile::parse(&String::from_utf8_lossy(&fs::read(&path)?))
        };
        if file.int("renderDistance") > 32 {
            file.set("renderDistance", V::Int(12));
        }
        write!(out, "== {name}\n{}", file.serialize())?;
        if first {
            first = false;
            writeln!(out, "== keys")?;
            for (name, key, category) in keybinds::MAPPINGS {
                writeln!(out, "{name} {key} {category} {}", keybinds::is_toggle(name))?;
            }
        }
    }
    Ok(())
}
