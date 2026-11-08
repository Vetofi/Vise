
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Who {
    Player,
    Mob(u32),
}
