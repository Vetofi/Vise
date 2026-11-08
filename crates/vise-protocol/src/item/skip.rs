use super::{Component, Template};
use crate::{
    Error, Result,
    buf::{Codec, Reader},
};

type Read = fn(&mut Reader) -> Result<()>;

pub(super) fn component(kind: i32, r: &mut Reader) -> Result<()> {
    match kind {
        34 | 120 => Ok(()),
        22 | 49 | 59 | 68 | 81 | 82 => nbt(r),
        8 | 31 | 42 | 43 | 44 | 46 | 65 | 75 | 88..=108 | 110..=117 | 121 => varint(r),
        7 | 54 => f32(r),
        35 | 73 => ident(r),
        5 => {
            r.bool()?;
            r.bool()?;
            f32(r)
        }
        14 | 15 => list(r, block_predicate),
        16 => list(r, attribute_modifier),
        23 => {
            varint(r)?;
            f32(r)?;
            boolean(r)
        }
        24 => {
            f32(r)?;
            varint(r)?;
            sound(r)?;
            boolean(r)?;
            list(r, consume_effect)
        }
        25 | 80 => template(r),
        26 => {
            f32(r)?;
            optional(r, ident)
        }
        27 | 33 | 67 => holder_set(r),
        28 => tool(r),
        29 => {
            varint(r)?;
            f32(r)
        }
        32 => equippable(r),
        36 => list(r, consume_effect),
        37 => blocks_attacks(r),
        38 => {
            boolean(r)?;
            boolean(r)?;
            optional(r, sound)?;
            optional(r, sound)
        }
        39 => kinetic_weapon(r),
        40 | 41 => {
            varint(r)?;
            varint(r)
        }
        52 => list(r, template),
        55 => list(r, |r| {
            varint(r)?;
            varint(r)
        }),
        58 => {
            trim_material(r)?;
            holder(r, |r| {
                ident(r)?;
                nbt(r)?;
                boolean(r)
            })
        }
        60 | 62 => {
            varint(r)?;
            nbt(r)
        }
        61 => nbt(r),
        63 => holder(r, |r| {
            sound(r)?;
            f32(r)?;
            f32(r)?;
            varint(r)?;
            nbt(r)
        }),
        64 => trim_material(r),
        66 => holder(r, |r| {
            sound(r)?;
            nbt(r)?;
            f32(r)?;
            varint(r)
        }),
        69 => {
            optional(r, |r| {
                ident(r)?;
                r.i64().map(drop)
            })?;
            boolean(r)
        }
        70 => firework_explosion(r),
        71 => {
            varint(r)?;
            list(r, firework_explosion)
        }
        74 => list(r, |r| {
            holder(r, |r| {
                ident(r)?;
                string(r)
            })?;
            varint(r)
        }),
        76 => (0..4).try_for_each(|_| optional(r, template)),
        77 => list(r, |r| optional(r, template)),
        78 => list(r, |r| {
            string(r)?;
            string(r)
        }),
        79 => list(r, |r| {
            varint(r)?;
            nbt(r)?;
            varint(r)?;
            varint(r)
        }),
        83 => sound(r),
        84 => resolvable(r, i32),
        85 | 86 => {
            resolvable(r, i32)?;
            resolvable(r, f32)
        }
        87 => {
            holder_set(r)?;
            f32(r)
        }
        109 => holder(r, |r| {
            varint(r)?;
            varint(r)?;
            ident(r)?;
            optional(r, nbt)?;
            optional(r, nbt)
        }),
        118 | 119 => {
            (0..4).try_for_each(|_| nbt(r))?;
            optional(r, |r| (0..4).try_for_each(|_| nbt(r)))?;
            varint(r)?;
            boolean(r)
        }
        other => Err(Error::new(format!("unsupported item component {other}"))),
    }
}

fn boolean(r: &mut Reader) -> Result<()> {
    r.bool().map(drop)
}
fn varint(r: &mut Reader) -> Result<()> {
    r.varint().map(drop)
}
fn f32(r: &mut Reader) -> Result<()> {
    r.f32().map(drop)
}
fn i32(r: &mut Reader) -> Result<()> {
    r.i32().map(drop)
}
fn string(r: &mut Reader) -> Result<()> {
    r.string(32767).map(drop)
}
fn ident(r: &mut Reader) -> Result<()> {
    string(r)
}
fn nbt(r: &mut Reader) -> Result<()> {
    r.nbt().map(drop)
}
fn template(r: &mut Reader) -> Result<()> {
    Template::read(r).map(drop)
}
fn optional(r: &mut Reader, value: Read) -> Result<()> {
    if r.bool()? { value(r) } else { Ok(()) }
}
fn list(r: &mut Reader, element: Read) -> Result<()> {
    let n = r.count(r.remaining())?;
    (0..n).try_for_each(|_| element(r))
}
fn holder(r: &mut Reader, direct: Read) -> Result<()> {
    if r.varint()? == 0 { direct(r) } else { Ok(()) }
}
fn holder_set(r: &mut Reader) -> Result<()> {
    match r.varint()? {
        0 => ident(r),
        n => {
            let n = usize::try_from(n - 1).map_err(|_| Error::new("negative holder set size"))?;
            if n > r.remaining() {
                return Err(Error::new("holder set past the end"));
            }
            (0..n).try_for_each(|_| varint(r))
        }
    }
}
fn sound(r: &mut Reader) -> Result<()> {
    holder(r, |r| {
        ident(r)?;
        optional(r, f32)
    })
}
fn resolvable(r: &mut Reader, constant: Read) -> Result<()> {
    if r.bool()? { constant(r) } else { ident(r) }
}
fn trim_material(r: &mut Reader) -> Result<()> {
    holder(r, |r| {
        ident(r)?;
        nbt(r)
    })
}
fn mob_effect(r: &mut Reader) -> Result<()> {
    varint(r)?;
    effect_details(r)
}
fn effect_details(r: &mut Reader) -> Result<()> {
    varint(r)?;
    varint(r)?;
    boolean(r)?;
    boolean(r)?;
    boolean(r)?;
    optional(r, effect_details)
}
fn consume_effect(r: &mut Reader) -> Result<()> {
    match r.varint()? {
        0 => {
            list(r, mob_effect)?;
            f32(r)
        }
        1 => holder_set(r),
        2 => Ok(()),
        3 => {
            f32(r)?;
            boolean(r)
        }
        4 => sound(r),
        other => Err(Error::new(format!("unknown consume effect type {other}"))),
    }
}
fn block_predicate(r: &mut Reader) -> Result<()> {
    optional(r, holder_set)?;
    optional(r, |r| {
        list(r, |r| {
            string(r)?;
            if r.bool()? {
                string(r)
            } else {
                optional(r, string)?;
                optional(r, string)
            }
        })
    })?;
    optional(r, nbt)?;
    list(r, |r| {
        let kind = r.varint()?;
        Component::read(kind, r).map(drop)
    })?;
    list(r, |r| {
        boolean(r)?;
        varint(r)?;
        nbt(r)
    })
}
fn attribute_modifier(r: &mut Reader) -> Result<()> {
    varint(r)?;
    ident(r)?;
    r.f64()?;
    varint(r)?;
    varint(r)?;
    match r.varint()? {
        0 | 1 => Ok(()),
        2 => nbt(r),
        other => Err(Error::new(format!(
            "unknown attribute modifier display {other}"
        ))),
    }
}
fn tool(r: &mut Reader) -> Result<()> {
    list(r, |r| {
        holder_set(r)?;
        optional(r, f32)?;
        optional(r, boolean)
    })?;
    f32(r)?;
    varint(r)?;
    boolean(r)
}
fn equippable(r: &mut Reader) -> Result<()> {
    varint(r)?;
    sound(r)?;
    optional(r, ident)?;
    optional(r, ident)?;
    optional(r, holder_set)?;
    (0..5).try_for_each(|_| boolean(r))?;
    sound(r)
}
fn blocks_attacks(r: &mut Reader) -> Result<()> {
    f32(r)?;
    f32(r)?;
    list(r, |r| {
        f32(r)?;
        optional(r, holder_set)?;
        f32(r)?;
        f32(r)
    })?;
    (0..3).try_for_each(|_| f32(r))?;
    optional(r, holder_set)?;
    optional(r, sound)?;
    optional(r, sound)
}
fn kinetic_weapon(r: &mut Reader) -> Result<()> {
    varint(r)?;
    varint(r)?;
    (0..3).try_for_each(|_| {
        optional(r, |r| {
            varint(r)?;
            f32(r)?;
            f32(r)
        })
    })?;
    f32(r)?;
    f32(r)?;
    optional(r, sound)?;
    optional(r, sound)
}
pub(super) fn potion_effects_and_name(r: &mut Reader) -> Result<()> {
    list(r, mob_effect)?;
    optional(r, string)
}
fn firework_explosion(r: &mut Reader) -> Result<()> {
    varint(r)?;
    list(r, i32)?;
    list(r, i32)?;
    boolean(r)?;
    boolean(r)
}
