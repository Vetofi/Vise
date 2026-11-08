
use std::collections::BTreeSet;

use crate::Pack;

const SOURCES: [&str; 8] = ["vsh", "fsh", "gsh", "csh", "tcs", "tes", "glsl", "inc"];

pub fn switches_on(pack: &Pack) -> BTreeSet<String> {
    let (mut on, mut off, mut tested) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let files = pack.all_files();
    let source = |path: &str| SOURCES.contains(&path.rsplit_once('.').map_or("", |(_, e)| e));
    let texts: Vec<(&String, String)> = files
        .iter()
        .filter(|p| source(p))
        .filter_map(|p| Some((p, pack.read_text(p)?)))
        .collect();
    let included: BTreeSet<&str> = texts
        .iter()
        .flat_map(|(_, t)| t.lines())
        .filter_map(|l| l.trim().strip_prefix("#include"))
        .map(|r| r.trim().trim_matches(|c| c == '"' || c == '<' || c == '>'))
        .map(|r| r.rsplit('/').next().unwrap_or(r))
        .collect();
    let extra: Vec<(&String, String)> = files
        .iter()
        .filter(|p| !source(p) && included.contains(p.rsplit('/').next().unwrap_or(p)))
        .filter_map(|p| Some((p, pack.read_text(p)?)))
        .collect();
    for (_, text) in texts.iter().chain(&extra) {
        for line in text.lines() {
            let line = line.trim();
            let (commented, rest) = match line.strip_prefix("//") {
                Some(rest) => (true, rest.trim_start()),
                None => (false, line),
            };
            let Some((directive, rest)) = rest
                .strip_prefix('#')
                .map(str::trim_start)
                .and_then(|r| r.split_once(|c: char| c.is_whitespace()))
            else {
                continue;
            };
            let mut words = rest.split_whitespace();
            let Some(name) = words.next() else { continue };
            match directive {
                "ifdef" | "ifndef" if !commented => _ = tested.insert(name.to_owned()),
                "define" if words.next().is_none_or(|w| w.starts_with("//")) => {
                    let set = if commented { &mut off } else { &mut on };
                    set.insert(name.to_owned());
                }
                _ => {}
            }
        }
    }
    on.retain(|name| tested.contains(name) && !off.contains(name));
    on
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::tests::zip;

    #[test]
    fn switches_need_a_test_and_no_value() {
        let pack = Pack::from_zip_bytes(
            "P.zip",
            zip(&[
                (
                    "shaders/lib/settings.glsl",
                    "  #define AO // ambient occlusion\n//#define BLOOM\n#define LEVEL 2 //[1 2]\n\
                     #define SPLIT\n#define UNTESTED\n",
                ),
                (
                    "shaders/world0/deferred.fsh",
                    "#ifdef AO\n#endif\n#ifndef BLOOM\n#endif\n#ifdef SPLIT\n#endif\n#ifdef LEVEL\n#endif\n",
                ),
                ("shaders/world-1/deferred.fsh", "//#define SPLIT\n"),
                (
                    "shaders/final.fsh",
                    "#include \"shaders.settings\"\n#ifdef GODRAYS\n#endif\n",
                ),
                ("shaders/shaders.settings", "    #define GODRAYS //Toggle\n"),
                ("shaders/notes.settings", "#define NOT_INCLUDED\n#ifdef NOT_INCLUDED\n"),
                ("shaders/shaders.properties", "#define NOT_SOURCE\n#ifdef NOT_SOURCE\n"),
            ]),
        )
        .unwrap();
        assert_eq!(
            switches_on(&pack),
            BTreeSet::from(["AO".to_owned(), "GODRAYS".to_owned()])
        );
    }
}
