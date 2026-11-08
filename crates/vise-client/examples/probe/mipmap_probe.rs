use std::{fmt::Write as _, fs, path::Path};

pub fn main() -> vise_client::Result<()> {
    let args: Vec<String> = crate::args().skip(1).collect();
    let input = fs::read_to_string(&args[0])?;
    let mut lines = input.lines();
    let root = Path::new(lines.next().ok_or("no resources root")?);
    let names: Vec<&str> = lines.collect();
    let mut sprites = vise_client::sprites::Sprites::default();
    let layers = names
        .iter()
        .map(|name| sprites.insert(root, name))
        .collect::<vise_client::Result<Vec<u32>>>()?;
    sprites.finish();
    let size = sprites.size;
    let bytes = (size * size * 4) as usize;
    let mut out = String::new();
    for (name, layer) in names.iter().zip(layers) {
        let start = layer as usize * bytes;
        let base = vise_client::mipmap::Image {
            width: size,
            height: size,
            pixels: sprites.pixels[start..start + bytes]
                .as_chunks::<4>()
                .0
                .to_vec(),
        };
        let s = sprites.mips[layer as usize];
        writeln!(out, "== {name}")?;
        for mip in vise_client::mipmap::generate(base, size.trailing_zeros(), s) {
            write!(out, "{}x{}", mip.width, mip.height)?;
            for row in mip.pixels.chunks(mip.width as usize) {
                out.push(' ');
                for p in row {
                    write!(out, "{:02x}{:02x}{:02x}{:02x}", p[0], p[1], p[2], p[3])?;
                }
            }
            out.push('\n');
        }
    }
    fs::write(&args[1], out)?;
    Ok(())
}
