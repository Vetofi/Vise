
use vise_api::{
    ActionResult, BlockDef, BlockSettings, ClientModInitializer, Command, Environment, Identifier,
    ItemDef, Mod, ModContext, ModInitializer, ModMetadata, Rarity, bridge, events, export,
};

struct Ruby;

impl ModInitializer for Ruby {
    fn on_initialize(&mut self, ctx: &mut ModContext<'_>) {
        let block = ctx.id("ruby_block");
        let _ = ctx.registries.blocks.register(
            block,
            BlockDef::new(
                Identifier::vanilla("amethyst_block"),
                BlockSettings::new()
                    .hardness(3.0)
                    .light(7)
                    .requires_tool(true),
            ),
        );
        let _ = ctx.registries.items.register(
            ctx.id("ruby"),
            ItemDef::new(Identifier::vanilla("diamond"))
                .max_stack_size(16)
                .rarity(Rarity::Rare),
        );

        events::BLOCK_BREAK.register(|b| {
            if b.pos[1] < -60 {
                return ActionResult::Fail;
            }
            ActionResult::Pass
        });
        events::ITEM_TOOLTIP.register(|t| {
            if t.item.id.namespace() == "ruby" {
                t.lines.push(events::TooltipLine::new("§cA very red stone"));
            }
        });
        events::END_SERVER_TICK.register(|_| {});
        events::PLAYER_JOIN.register(|p| println!("{} joined", p.name));
        events::PLAYER_LEAVE.register(|l| println!("{} left: {}", l.player.name, l.reason));
        events::PLAYER_CHAT.register(|c| println!("<{}> {}", c.player.name, c.text));

        vise_api::COMMAND_REGISTRATION.register(|commands| {
            let cmd = Command::new("ruby [text:string]")
                .unwrap()
                .executes(|c| c.reply(format!("{} says: {}", c.player.name, c.args)));
            let _ = commands.register(cmd);
        });
    }
}

impl ClientModInitializer for Ruby {
    fn on_initialize_client(&mut self, _ctx: &mut ModContext<'_>) {
        events::KEY_INPUT.register(|k| {
            if k.pressed && k.key == "key.keyboard.r" {
                println!("R pressed");
            }
        });
    }
}

fn ruby_mod() -> Mod {
    Mod::new(
        ModMetadata::new("ruby", "1.0.0")
            .name("Ruby")
            .author("n1kutin"),
    )
    .main(Ruby)
    .client(Ruby)
}

fn main() {
    bridge::add_mod(ruby_mod()).unwrap();
    let loaded = bridge::init(Environment::Client).as_ref().unwrap();
    println!("loaded: {loaded:?}");

    println!(
        "break at y=-64 cancelled: {}",
        bridge::destroy_block([0, -64, 0], 1)
    );
    println!(
        "break at y=70 cancelled: {}",
        bridge::destroy_block([0, 70, 0], 1)
    );

    bridge::with_registries(|r| {
        for f in export::content_files("ruby", r).unwrap() {
            println!("--- {}\n{}", f.path, f.text);
        }
    });
    println!(
        "--- vise-mod.toml\n{}",
        export::manifest_toml(
            &ModMetadata::new("ruby", "1.0.0")
                .name("Ruby")
                .author("n1kutin")
        )
    );
}
