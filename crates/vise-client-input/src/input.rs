
fn length(v: [f32; 2]) -> f32 {
    f64::from(v[0] * v[0] + v[1] * v[1]).sqrt() as f32
}
fn scale(v: [f32; 2], factor: f32) -> [f32; 2] {
    [v[0] * factor, v[1] * factor]
}
pub fn keyboard_direction(forward: bool, backward: bool, left: bool, right: bool) -> [f32; 2] {
    let impulse = |positive: bool, negative: bool| {
        if positive == negative {
            0.0
        } else if positive {
            1.0
        } else {
            -1.0
        }
    };
    let v = [impulse(left, right), impulse(forward, backward)];
    let magnitude = length(v);
    if magnitude < 1.0e-4 {
        [0.0, 0.0]
    } else {
        [v[0] / magnitude, v[1] / magnitude]
    }
}

pub fn modify_input(input: [f32; 2], item_use: Option<f32>, sneaking: Option<f32>) -> [f32; 2] {
    if input[0] * input[0] + input[1] * input[1] == 0.0 {
        return input;
    }
    let mut v = scale(input, 0.98);
    if let Some(multiplier) = item_use {
        v = scale(v, multiplier);
    }
    if let Some(multiplier) = sneaking {
        v = scale(v, multiplier);
    }
    square_movement(v)
}

fn square_movement(v: [f32; 2]) -> [f32; 2] {
    let magnitude = length(v);
    if magnitude <= 0.0 {
        return v;
    }
    let unit = scale(v, 1.0 / magnitude);
    let x = unit[0].abs();
    let y = unit[1].abs();
    let ratio = if y > x { x / y } else { y / x };
    let distance = f64::from(1.0 + ratio * ratio).sqrt() as f32;
    let speed = magnitude * distance;
    scale(
        unit,
        if speed.is_nan() || speed < 1.0 {
            speed
        } else {
            1.0
        },
    )
}
