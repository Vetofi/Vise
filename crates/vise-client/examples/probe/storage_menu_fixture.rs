use vise_client::containers::block_menu_type;
use std::fs;

fn case(block: &str) -> Option<String> {
    let menu = block_menu_type(block.trim())?;
    let accepts = if menu == "minecraft:shulker_box" {
        "refuses"
    } else {
        "accepts"
    };
    Some(format!("{menu} {accepts} shulker_box"))
}

pub fn main() -> vise_client::Result<()> {
    let args: Vec<String> = crate::args().collect();
    let input = fs::read_to_string(
        args.get(1)
            .ok_or("usage: storage_menu_fixture INPUT OUTPUT")?,
    )?;
    let lines: Vec<String> = input
        .lines()
        .map(|line| case(line).unwrap_or_else(|| "error".to_owned()))
        .collect();
    fs::write(
        args.get(2).ok_or("missing OUTPUT")?,
        lines.join("\n") + "\n",
    )?;
    Ok(())
}
