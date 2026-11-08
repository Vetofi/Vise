use vise_core::position::BlockPos;
use vise_world::nbt::Tag;
use serde_json::Value;

pub fn f64_bits(v: f64) -> u64 {
    if v.is_nan() {
        f64::NAN.to_bits()
    } else {
        v.to_bits()
    }
}

pub fn f32_bits(v: f32) -> u32 {
    if v.is_nan() {
        f32::NAN.to_bits()
    } else {
        v.to_bits()
    }
}

pub fn f64_hex(v: f64) -> String {
    format!("{:016x}", f64_bits(v))
}

pub fn tag(v: &Value) -> Tag {
    match v {
        Value::String(s) => Tag::String(s.as_str().into()),
        Value::Number(n) => Tag::Int(n.as_i64().unwrap() as i32),
        Value::Array(a) => Tag::List(a.iter().map(tag).collect()),
        Value::Object(m) => {
            Tag::Compound(m.iter().map(|(k, v)| (k.as_str().into(), tag(v))).collect())
        }
        _ => panic!("invalid registry metadata"),
    }
}

pub fn position(v: &Value) -> BlockPos {
    BlockPos::new(
        v[0].as_i64().unwrap() as i32,
        v[1].as_i64().unwrap() as i32,
        v[2].as_i64().unwrap() as i32,
    )
}
