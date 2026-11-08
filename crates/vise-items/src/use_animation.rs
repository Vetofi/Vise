
pub fn consumable_animation(v: &serde_json::Value) -> vise_world::consumable::UseAnimation {
    use vise_world::consumable::UseAnimation;
    match v
        .get("animation")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("eat")
    {
        "eat" => UseAnimation::Eat,
        "drink" => UseAnimation::Drink,
        _ => UseAnimation::None,
    }
}
