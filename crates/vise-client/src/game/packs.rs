use super::{
    Game, PackResources, Resources, UiResources, build_renderer, prepare_renderer_textures,
};
use vise_client::{
    assets::Assets,
    gfx::PreparedTextures,
    hud::HudAtlas,
    lang,
    pack_screen::{self, Compat, PackInfo, Source as PackSource},
    packs::{self, Pack, Position},
    streaming::WorkerContext,
    text::{Style, Text},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn cache_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".vise-cache/resourcepacks")
}
pub fn packs_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("resourcepacks")
}
pub fn open_folder(dir: &Path) {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(dir).spawn() {
        log::warn!("Vise: could not open {}: {e}", dir.display());
    }
}
pub fn open_uri(uri: &str) {
    #[cfg(windows)]
    if let Err(error) = open::that_detached(uri) {
        log::warn!("Vise: could not open URI {uri}: {error}");
    }
    #[cfg(not(windows))]
    open_folder(Path::new(uri));
}
pub fn startup_root(
    vanilla: &Path,
    selected: &[String],
    server: &[Pack],
) -> std::io::Result<PathBuf> {
    let all = packs::discover(vanilla, &packs_dir());
    let mut enabled = packs::enabled(&all, selected);
    enabled.extend_from_slice(server);
    packs::resource_root(vanilla, &enabled, &cache_dir())
}
fn info(p: &Pack) -> PackInfo {
    let built_in = !p.id.starts_with("file/");
    PackInfo {
        id: p.id.clone(),
        title: p.title.clone(),
        description: p.raw_description.clone(),
        description_style: p.description_style.clone(),
        source: if built_in {
            PackSource::BuiltIn
        } else {
            PackSource::Default
        },
        compat: p.compatibility,
        icon: p.icon_png().as_deref().and_then(pack_screen::load_icon),
        required: false,
        position: Position::Top,
        fixed: false,
    }
}
fn vanilla_info(vanilla: &Path) -> PackInfo {
    PackInfo {
        id: "vanilla".into(),
        title: Text::translate("resourcePack.vanilla.name", &[]),
        description: Text::translate("resourcePack.vanilla.description", &[]),
        description_style: Style::default(),
        source: PackSource::BuiltIn,
        compat: Compat::Compatible,
        icon: std::fs::read(vanilla.join("pack.png"))
            .ok()
            .as_deref()
            .and_then(pack_screen::load_icon),
        required: true,
        position: Position::Bottom,
        fixed: false,
    }
}
pub(super) fn pack_infos(vanilla: &Path) -> Vec<PackInfo> {
    let mut infos: Vec<PackInfo> = packs::discover(vanilla, &packs_dir())
        .iter()
        .map(info)
        .collect();
    infos.push(vanilla_info(vanilla));
    infos
}
impl Game<'_> {
    pub(super) fn vanilla_root(&self) -> PathBuf {
        if self.ui.vanilla_root.as_os_str().is_empty() {
            self.options.resources.clone()
        } else {
            self.ui.vanilla_root.clone()
        }
    }
    pub(super) fn refresh_pack_lists(&mut self) {
        let infos = pack_infos(&self.vanilla_root());
        let o = &mut self.ui.options;
        o.pack_infos = infos;
        o.pack_selection = self.settings.resource_packs.clone();
        o.pack_screen = None;
    }
    pub(super) fn reload_packs(&mut self) {
        if reload_waits(
            self.rest_loader.is_some() || self.world_loader.is_some(),
            self.pack_reload.is_some(),
        ) {
            self.pending_pack_reload = true;
            return;
        }
        if let Err(e) = self.spawn_pack_reload(ReloadKind::Selection) {
            self.selection_reload_failed(&format!("could not start the pack reload: {e}"));
        }
    }
    fn selection_reload_failed(&mut self, error: &str) {
        log::warn!("Vise: resource reload failed: {error}");
        if self.screen != super::Screen::Options {
            self.open_options_at(
                self.title.is_some(),
                vise_client::options_ui::Page::Packs,
            );
        }
        self.ui.options.resource_error = Some(error.to_string());
    }
    pub(super) fn spawn_pack_reload(&mut self, kind: ReloadKind) -> std::io::Result<()> {
        self.keys.clear();
        self.physical_keys.clear();
        self.left = false;
        self.right = false;
        let vanilla = self.vanilla_root();
        if self.ui.vanilla_root.as_os_str().is_empty() {
            self.ui.vanilla_root = vanilla.clone();
        }
        let (selected, server, mods, dev_mods) = (
            self.settings.resource_packs.clone(),
            self.server_packs.active.clone(),
            self.settings.mods.enabled.clone(),
            self.options.dev_mods.clone(),
        );
        let (assets, res, context) = (
            self.pack.assets.clone(),
            self.res.clone(),
            self.pack.context.clone(),
        );
        let (unicode, japanese) = (self.settings.force_unicode, self.settings.japanese_glyphs);
        self.pack_reload = Some(PackReload::spawn(kind, move |progress| {
            stage_pack(
                &vanilla, &selected, &server, &mods, &dev_mods, &assets, &res, &context, unicode,
                japanese, progress,
            )
            .map_err(|e| e.to_string())
        })?);
        Ok(())
    }
    pub(super) fn drive_pack_reload(&mut self) {
        let Some((kind, staged)) = self.pack_reload.as_mut().and_then(PackReload::finished) else {
            return;
        };
        self.pack_reload = None;
        let result =
            staged.and_then(|staged| self.apply_staged_pack(staged).map_err(|e| e.to_string()));
        match kind {
            ReloadKind::Selection => {
                if let Err(e) = result {
                    self.selection_reload_failed(&e);
                }
            }
            ReloadKind::Server => {
                if let Err(e) = &result {
                    log::warn!("Vise: server resource pack reload failed: {e}");
                }
                self.finish_pack_reload(result.is_ok());
            }
        }
    }
    pub(super) fn apply_staged_pack(&mut self, staged: StagedPack) -> vise_client::Result<()> {
        let StagedPack {
            root,
            gui,
            pack,
            textures,
        } = staged;
        if let Some(renderer) = &mut self.renderer {
            renderer.release_surface();
        }
        let renderer = match self
            .window
            .clone()
            .map(|window| {
                build_renderer(
                    window,
                    self.options.offscreen,
                    &pack,
                    &gui,
                    self.renderer.as_ref(),
                    Some(textures),
                    self.settings.vise.api,
                )
            })
            .transpose()
        {
            Ok(renderer) => renderer,
            Err(error) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.recreate_surface().map_err(|restore| {
                        format!(
                            "{error}; could not restore the previous rendering surface: {restore}"
                        )
                    })?;
                }
                return Err(error);
            }
        };
        self.waypoint_styles = vise_client::locator_bar::WaypointStyles::load(&root);
        self.armor_stand_assets.reload_trims(&root);
        self.random_tick_sections.clear();
        self.portals.sections.clear();
        self.options.resources = root.clone();
        let context = pack.context.clone();
        self.pack = pack;
        self.gui = gui;
        self.particles
            .reload(&root, &self.gui.atlas.sprites, &self.gui.atlas.translucent);
        vise_client::ui_theme::reset_atlas();
        *self
            .res
            .mob_data()
            .villager_hats
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            vise_client::mobs::villager_hats::VillagerHats::load(&root);
        vise_client::ui::load_metadata(&root);
        lang::set(lang::Language::load(&[&root], &lang::code()));
        self.refresh_remote_language();
        self.audio.reload(&root);
        self.clear_lod();
        self.streaming.reload_resources(&mut self.world, context);
        self.pending_uploads.clear();
        if let Some(renderer) = renderer {
            self.renderer = Some(renderer);
            self.apply_ui_settings();
        }
        self.ui.chat.rescale(&self.gui.atlas);
        self.creative_screen.forget_items();
        Ok(())
    }
}

pub(super) fn stage_pack(
    vanilla_root: &Path,
    selected: &[String],
    server: &[Pack],
    mods_enabled: &BTreeSet<String>,
    dev_mods: &[PathBuf],
    assets: &Arc<Assets>,
    res: &Resources,
    context: &WorkerContext,
    force_unicode: bool,
    japanese: bool,
    progress: &super::Progress,
) -> vise_client::Result<StagedPack> {
    let root = startup_root(vanilla_root, selected, server)?;
    let root = super::mods::content_root(&root, mods_enabled, dev_mods);
    progress.set(1);
    let render = Arc::new(assets.reload(&root)?);
    let mut atlas = HudAtlas::build_over(&root, vanilla_root)?;
    atlas.font.uniform = force_unicode;
    atlas.font.japanese = japanese;
    let gui = UiResources {
        atlas,
        panorama: Arc::new(vise_client::gfx::panorama::Faces::load(&root)),
    };
    progress.set(2);
    let pack = PackResources::load(
        &root,
        assets.clone(),
        render,
        res.facts(),
        context.biomes.clone(),
        context.light.clone(),
    )?;
    let textures = prepare_renderer_textures(&pack);
    progress.set(3);
    Ok(StagedPack {
        root,
        gui,
        pack,
        textures,
    })
}

pub(super) struct StagedPack {
    pub root: PathBuf,
    pub gui: UiResources,
    pub pack: PackResources,
    pub textures: PreparedTextures,
}

const RELOAD_LINES: [&str; 4] = [
    "Preparing resources",
    "Loading assets",
    "Loading pack tables",
    "Applying resources",
];
fn reload_waits(world_loading: bool, in_flight: bool) -> bool {
    world_loading || in_flight
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReloadKind {
    Selection,
    Server,
}
type StageResult = std::result::Result<StagedPack, String>;
pub(super) struct PackReload {
    pub(super) progress: super::Progress,
    kind: ReloadKind,
    handle: Option<std::thread::JoinHandle<StageResult>>,
}
impl PackReload {
    fn spawn(
        kind: ReloadKind,
        stage: impl FnOnce(&super::Progress) -> StageResult + Send + 'static,
    ) -> std::io::Result<Self> {
        let progress = super::Progress::with(&RELOAD_LINES);
        let worker = progress.clone();
        let handle = std::thread::Builder::new()
            .name("pack-reload".into())
            .spawn(move || stage(&worker))?;
        Ok(Self {
            progress,
            kind,
            handle: Some(handle),
        })
    }
    pub(super) fn kind(&self) -> ReloadKind {
        self.kind
    }
    fn finished(&mut self) -> Option<(ReloadKind, StageResult)> {
        if !self.handle.as_ref()?.is_finished() {
            return None;
        }
        let staged = self
            .handle
            .take()?
            .join()
            .unwrap_or_else(|_| Err("the pack reload worker panicked".into()));
        Some((self.kind, staged))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    fn wait(reload: &mut PackReload) -> (ReloadKind, StageResult) {
        for _ in 0..500 {
            if let Some(done) = reload.finished() {
                return done;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the reload worker never finished");
    }

    #[test]
    fn a_reload_stages_on_a_worker_while_the_caller_goes_on() {
        let (release, blocked) = mpsc::channel::<()>();
        let (thread, ran_on) = mpsc::channel();
        let mut reload = PackReload::spawn(ReloadKind::Selection, move |progress| {
            blocked.recv().unwrap();
            progress.set(2);
            thread
                .send(std::thread::current().name().map(str::to_owned))
                .unwrap();
            Err("staged".into())
        })
        .unwrap();
        assert!(reload.finished().is_none(), "still staging");
        assert_eq!(reload.progress.line(), "Preparing resources");
        release.send(()).unwrap();
        let (kind, staged) = wait(&mut reload);
        assert_eq!(kind, ReloadKind::Selection);
        assert_eq!(staged.err().as_deref(), Some("staged"));
        assert_eq!(reload.progress.line(), "Loading pack tables");
        assert_eq!(ran_on.recv().unwrap().as_deref(), Some("pack-reload"));
        assert!(reload.finished().is_none(), "the outcome is taken once");
    }

    #[test]
    fn a_panicking_stage_fails_the_reload() {
        let mut reload = PackReload::spawn(ReloadKind::Server, |_| panic!("bad pack")).unwrap();
        let (kind, staged) = wait(&mut reload);
        assert_eq!(kind, ReloadKind::Server);
        assert_eq!(
            staged.err().as_deref(),
            Some("the pack reload worker panicked")
        );
    }

    #[test]
    fn a_reload_waits_for_world_resources_and_the_one_in_flight() {
        assert!(!reload_waits(false, false));
        assert!(reload_waits(true, false));
        assert!(reload_waits(false, true));
        assert!(reload_waits(true, true));
    }
}
