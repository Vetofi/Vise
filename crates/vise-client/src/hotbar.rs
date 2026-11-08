use crate::{items::ItemStack, save};
use vise_world::nbt::{JavaString, Tag};
use std::path::Path;

pub const GROUPS: usize = 9;
pub const SIZE: usize = 9;

pub type Hotbar = Vec<Option<ItemStack>>;

fn empty() -> Hotbar {
    vec![None; SIZE]
}

fn row(tag: Option<&Tag>, known: &dyn Fn(&str) -> bool) -> Hotbar {
    let Some(Tag::List(items)) = tag else {
        return empty();
    };
    if items.len() < SIZE {
        return empty();
    }
    items.iter().take(SIZE).map(|t| stack(t, known)).collect()
}

fn stack(tag: &Tag, known: &dyn Fn(&str) -> bool) -> Option<ItemStack> {
    let stack = save::item_from_nbt(tag)?;
    if !known(&stack.item) {
        return None;
    }
    let count = match tag.get("count") {
        Some(Tag::Byte(n)) => i32::from(*n),
        Some(Tag::Short(n)) => i32::from(*n),
        Some(Tag::Int(n)) => *n,
        _ => 1,
    };
    Some(ItemStack {
        count: if (1..=99).contains(&count) {
            count as u32
        } else {
            1
        },
        ..stack
    })
}

pub fn from_tag(mut tag: Tag, known: &dyn Fn(&str) -> bool) -> Vec<Hotbar> {
    let version = match tag.get("DataVersion") {
        Some(Tag::Int(v)) => *v,
        _ => 1343,
    };
    const LAST_STACK_FIXER: i32 = 5016;
    if version < vise_world::DATA_VERSION
        && let Tag::Compound(rows) = &mut tag
    {
        if version < LAST_STACK_FIXER {
            let broken = (0..GROUPS).find(|i| {
                rows.get(&JavaString::from(i.to_string().as_str()))
                    .is_some_and(|r| {
                        !matches!(
                            r,
                            Tag::List(_) | Tag::ByteArray(_) | Tag::IntArray(_) | Tag::LongArray(_)
                        )
                    })
            });
            if let Some(broken) = broken {
                for i in broken..GROUPS {
                    rows.remove(&JavaString::from(i.to_string().as_str()));
                }
            }
            for row in rows.values_mut() {
                if let Tag::List(items) = row {
                    items.retain(|i| {
                        matches!(i, Tag::Compound(_))
                            || (version > 3818 && matches!(i, Tag::String(_)))
                    });
                }
            }
        } else {
            for row in rows.values_mut() {
                if let Tag::List(items) = row {
                    for item in items.iter_mut().filter(|i| matches!(i, Tag::String(_))) {
                        *item = Tag::Compound(Default::default());
                    }
                }
            }
        }
    }
    vise_world::datafix::update_hotbar(&mut tag);
    (0..GROUPS)
        .map(|i| row(tag.get(&i.to_string()), known))
        .collect()
}

pub fn load(path: &Path, known: &dyn Fn(&str) -> bool) -> Vec<Hotbar> {
    match save::read_dat(path) {
        Some(tag @ Tag::Compound(_)) => from_tag(tag, known),
        _ => (0..GROUPS).map(|_| empty()).collect(),
    }
}

pub fn save(path: &Path, rows: &[Hotbar]) -> crate::Result<()> {
    let mut entries = vec![(
        JavaString::from("DataVersion"),
        Tag::Int(vise_world::DATA_VERSION),
    )];
    for (i, row) in rows.iter().enumerate() {
        let stacks = row.iter().map(|s| match s {
            Some(s) => save::item_to_nbt(s),
            None => Tag::compound(Vec::<(&str, Tag)>::new()),
        });
        entries.push((
            JavaString::from(i.to_string().as_str()),
            Tag::List(stacks.collect()),
        ));
    }
    save::write_dat(path, Tag::compound(entries))
}
