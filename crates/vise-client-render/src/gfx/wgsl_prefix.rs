
pub const TERRAIN: &str = include_str!("../terrain.wgsl");

pub fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let from = source
        .find(&format!("\n{start}"))
        .expect("the source has the section")
        + 1;
    let to = from
        + source[from..]
            .find(&format!("\n{end}"))
            .expect("the source ends the section")
        + 1;
    &source[from..to]
}

fn declared(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for line in text.lines() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let mut line = line;
        while let Some(rest) = line.strip_prefix('@') {
            let end = rest.find(' ').unwrap_or(rest.len());
            line = rest[end..].trim_start();
        }
        let rest = ["fn ", "struct ", "const ", "override ", "var "]
            .iter()
            .find_map(|k| line.strip_prefix(k))
            .or_else(|| {
                let rest = line.strip_prefix("var<")?;
                Some(rest[rest.find('>')? + 1..].trim_start())
            });
        if let Some(rest) = rest {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            names.push(&rest[..end]);
        }
    }
    names
}

pub fn prefixed(text: &str, prefix: &str) -> String {
    let names = declared(text);
    let mut out = String::with_capacity(text.len() * 2);
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if names.contains(&word.as_str()) {
            out.push_str(prefix);
        }
        out.push_str(word);
        word.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_top_level_names_and_their_uses_only() {
        let text = "struct S { a: f32, }\n\
            @group(0) @binding(0) var<uniform> g: S;\n\
            @group(1) @binding(2) var<storage, read> list: array<u32>;\n\
            override N: u32 = 1u;\n\
            const K: f32 = 2.0;\n\
            @fragment fn f(x: f32) -> @location(0) vec4<f32> {\n    \
                var a = g.a * K;\n    \
                return vec4(a + f32(list[N]) + x);\n\
            }\n";
        let out = prefixed(text, "p_");
        assert_eq!(
            out,
            "struct p_S { a: f32, }\n\
            @group(0) @binding(0) var<uniform> p_g: p_S;\n\
            @group(1) @binding(2) var<storage, read> p_list: array<u32>;\n\
            override p_N: u32 = 1u;\n\
            const p_K: f32 = 2.0;\n\
            @fragment fn p_f(x: f32) -> @location(0) vec4<f32> {\n    \
                var a = p_g.a * p_K;\n    \
                return vec4(a + f32(p_list[p_N]) + x);\n\
            }\n"
        );
        let names = declared(TERRAIN);
        for name in [
            "Globals",
            "globals",
            "sprites_7",
            "SPRITE_SPLIT",
            "block_slots",
            "fs_sky",
        ] {
            assert!(names.contains(&name), "{name}");
        }
        assert!(!names.contains(&"out"));
    }
}
