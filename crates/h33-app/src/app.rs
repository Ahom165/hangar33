//! État de jeu + boucle : winit (entrées), fixed-timestep (sim 60 Hz),
//! rendu (wgpu + egui), script smoke-test.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use h33_core::grid::{CellContent, Dir, cell_key};
use h33_core::items::ItemKind;
use h33_core::machines::MachineKind;
use h33_core::rng::generate_master_seed;
use h33_core::shop::{ShopItem, ALL_SHOP_ITEMS};
use h33_core::sim::{GameEvent, Severity, Sim};
use h33_render::assets::GpuAssets;
use h33_render::camera::FpCamera;
use h33_render::surface::SurfaceRenderer;
use h33_render::types::{CameraUniform, InstanceRaw};

use crate::ui;

// ======================================================================
//  Options CLI (parsing dans main.rs)
// ======================================================================

#[derive(Default)]
pub struct AppOptions {
    pub total_packages: Option<u64>,
    pub seed: Option<[u8; 32]>,
    pub smoke_test: bool,
    pub screenshot: Option<PathBuf>,
    pub frames: Option<u64>,
}

pub struct App {
    pub opts: AppOptions,
    pub state: Option<GameState>,
}

impl App {
    pub fn new(opts: AppOptions) -> Self {
        Self { opts, state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("HANGAR 33 — v0.3.7"))
                .expect("création de fenêtre"),
        );
        let mut state = pollster::block_on(GameState::new(window, &self.opts))
            .expect("initialisation du jeu");
        // Préremplissage du menu (--total).
        if let Some(total) = self.opts.total_packages {
            state.menu_packages = (total as f64).log10();
        }
        self.state = Some(state);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if let Some(state) = self.state.as_mut() {
            state.handle_window_event(event_loop, window_id, event);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(state) = self.state.as_mut() {
            state.handle_device_event(event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Règle idle : AUCUNE progression hors-ligne — tout s'arrête ici.
        println!("[h33] fermeture — 0 progression hors-ligne (règle du design)");
    }
}

// ======================================================================
//  Mode de construction : l'argent se paie à l'ORDINATEUR (boutique
//  HANGAR-OS, livraison différée) ; ici on ne fait que POSER le stock.
// ======================================================================

/// Position MONDE de l'ordinateur de gestion (bureau sud-ouest, près du
/// dock de palettes). Constante partagée rendu + interaction.
pub const COMPUTER_POS: glam::Vec3 = glam::Vec3::new(-18.0, 0.0, 21.0);

/// État UI de l'ordinateur : un FAUX BUREAU « HANGAR-OS 11 » (clin d'œil
/// Windows 11) — barre des tâches, menu Démarrer, fenêtres d'apps.
#[derive(Default)]
pub struct ComputerUi {
    pub open: bool,
    /// Apps ouvertes du bureau : [Boutique, Colis, Terminal].
    pub apps: [bool; 3],
    /// Menu Démarrer déplié.
    pub start_open: bool,
    /// Fenêtre « Corbeille » (easter egg).
    pub trash_open: bool,
    /// Fenêtre « Ce PC » (easter egg).
    pub pc_open: bool,
    /// Recherche du menu Démarrer (décorative).
    pub start_search: String,
    /// Demandé par le bureau (bouton Marche/Arrêt) : fermer la session.
    pub close_request: bool,
    pub input: String,
    pub log: Vec<String>,
    pub order_qty: u32,
    pub package_qty: u64,
    pub greeted: bool,
}

impl ComputerUi {
    pub fn new() -> Self {
        Self {
            open: false,
            apps: [false; 3],
            start_open: false,
            trash_open: false,
            pc_open: false,
            start_search: String::new(),
            close_request: false,
            input: String::new(),
            log: vec![
                "HANGAR-OS 33 (tty1) — Terminal de gestion du hangar".into(),
                "Connecté : operateur@hangar33".into(),
                "Tape 'help' pour les commandes.".into(),
                "[ADMIN] pense-bête scotché à l'écran : « le code de la baie".into(),
                "serveur est le nom du jeu + le numéro du jeu »".into(),
            ],
            order_qty: 1,
            package_qty: 10,
            greeted: false,
        }
    }
}

// ======================================================================
//  Données de scène : instances des lots GLB (une Vec par asset)
// ======================================================================

pub struct SceneBatchData {
    pub shell: Vec<InstanceRaw>,
    pub unboxer: Vec<InstanceRaw>,
    pub seller: Vec<InstanceRaw>,
    pub incinerator: Vec<InstanceRaw>,
    pub splitter: Vec<InstanceRaw>,
    pub scanner: Vec<InstanceRaw>,
    pub autobuyer: Vec<InstanceRaw>,
    pub robotarm: Vec<InstanceRaw>,
    pub generator: Vec<InstanceRaw>,
    pub package: Vec<InstanceRaw>,
    pub computer: Vec<InstanceRaw>,
    // Meshes procéduraux du déballage (sommets blancs tintés par instance).
    pub open_crate: Vec<InstanceRaw>,
    pub prism: Vec<InstanceRaw>,
    pub sphere: Vec<InstanceRaw>,
    pub torus: Vec<InstanceRaw>,
}

// ======================================================================
//  État de jeu
// ======================================================================

pub struct GameState {
    pub window: Arc<Window>,
    pub renderer: SurfaceRenderer,
    pub egui_ctx: egui::Context,
    pub egui_winit: egui_winit::State,
    pub egui_renderer: egui_wgpu::Renderer,

    pub camera: FpCamera,
    /// Meshes GLB (Blender MCP) uploadés une fois au boot.
    pub assets: GpuAssets,
    pub sim: Option<Sim>,
    pub seed_fp: String,
    pub menu_packages: f64, // log10 du nombre de colis (slider)

    // Options reportées
    pub seed_override: Option<[u8; 32]>,
    pub smoke_test: bool,
    pub screenshot: Option<PathBuf>,
    pub frames_limit: Option<u64>,

    // Entrées
    pub cursor: (f32, f32),
    pub prev_cursor: (f32, f32),
    /// Pointer lock (vue FP) : souris capturée pour le mouse look.
    pub pointer_locked: bool,
    pub keys: HashSet<String>,
    pub hovered_cell: Option<(i32, i32)>,
    pub egui_consumed_mouse: bool,

    // Construction
    pub build_kind: Option<ShopItem>,
    pub belt_dir: Dir,
    pub demolish_mode: bool,
    pub selected: Option<usize>,

    // UI
    pub toasts: Vec<(String, Severity, f64)>,
    pub paused: bool,
    pub show_help: bool,
    pub fps_avg: f32,
    /// Demande posée par l'UI (menu / victoire), consommée chaque frame.
    pub start_request: Option<u64>,
    pub back_to_menu: bool,

    /// Ordinateur de gestion (HANGAR-OS) : état UI + proximité.
    pub computer: ComputerUi,
    /// Le joueur est à portée de l'ordinateur cette frame ?
    pub near_computer: bool,
    /// Prompt contextuel du colis le plus proche (touche F), calculé par frame.
    pub floor_prompt: Option<String>,

    // Capture de frame (smoke-test / debug) — chemin + conversion en vol.
    pub capture_request: Option<PathBuf>,
    pub last_capture_path: Option<PathBuf>,
    /// Capture dédiée de l'écran HANGAR-OS (2e capture du smoke-test).
    pub terminal_screenshot: Option<PathBuf>,

    // Boucle
    last_frame: Instant,
    acc: f32,
    pub frame: u64,
    /// Étape du script smoke-test.
    pub smoke_step: usize,
}

const FIXED_DT: f32 = 1.0 / 60.0;
const MAX_STEPS_PER_FRAME: u32 = 5;

impl GameState {
    pub async fn new(window: Arc<Window>, opts: &AppOptions) -> Result<Self, String> {
        let renderer = SurfaceRenderer::new(window.clone()).await?;
        let egui_ctx = egui::Context::default();
        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            Some(winit::window::Theme::Dark),
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(
            &renderer.gpu.device,
            renderer.scene.color_format(),
            None,
            1,
            true, // dithering anti-banding
        );

        let seed_fp = h33_core::rng::seed_fingerprint(&opts.seed.unwrap_or_else(generate_master_seed));

        // Assets Blender (GLB embarqués) — une seule fois, au boot.
        let assets = GpuAssets::load(&renderer.gpu.device);

        Ok(Self {
            window,
            renderer,
            egui_ctx,
            egui_winit,
            egui_renderer,
            camera: FpCamera::default(),
            assets,
            sim: None,
            seed_fp,
            menu_packages: 6.0,
            seed_override: opts.seed,
            smoke_test: opts.smoke_test,
            screenshot: opts.screenshot.clone(),
            frames_limit: opts.frames,
            cursor: (0.0, 0.0),
            prev_cursor: (0.0, 0.0),
            pointer_locked: false,
            keys: HashSet::new(),
            hovered_cell: None,
            egui_consumed_mouse: false,
            build_kind: None,
            belt_dir: Dir::East,
            demolish_mode: false,
            selected: None,
            toasts: Vec::new(),
            paused: false,
            show_help: false,
            fps_avg: 60.0,
            start_request: None,
            back_to_menu: false,
            computer: ComputerUi::new(),
            near_computer: false,
            floor_prompt: None,
            capture_request: None,
            last_capture_path: None,
            terminal_screenshot: opts.screenshot.as_ref().map(|p| {
                let mut s = p.to_path_buf();
                s.set_extension("terminal.png");
                s
            }),
            last_frame: Instant::now(),
            acc: 0.0,
            frame: 0,
            smoke_step: 0,
        })
    }

    /// Démarre une partie (menu -> jeu). La seed vient du TRNG
    /// (`generate_master_seed`) ou de l'override CLI (--seed).
    pub fn start_game(&mut self, total: u64) {
        let seed = self.seed_override.unwrap_or_else(generate_master_seed);
        self.seed_fp = h33_core::rng::seed_fingerprint(&seed);
        let total = total.clamp(h33_core::balance::MIN_PACKAGES, h33_core::balance::MAX_PACKAGES);
        println!("[h33] nouvelle partie — colis: {total} — seed: {}", self.seed_fp);
        self.sim = Some(Sim::new(seed, total));
        self.toasts.clear();
        self.build_kind = None;
        self.demolish_mode = false;
        self.selected = None;
        // Vue FP : capture de la souris dès l'entrée en jeu.
        if !self.smoke_test {
            self.lock_pointer();
        }
    }

    pub fn push_toast(&mut self, text: impl Into<String>, severity: Severity) {
        let now = self.sim.as_ref().map_or(0.0, |s| s.elapsed_s);
        self.toasts.push((text.into(), severity, now + 4.5));
        if self.toasts.len() > 8 {
            self.toasts.remove(0);
        }
    }

    // ------------------------------------------------------------------
    //  Événements fenêtre
    // ------------------------------------------------------------------

    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Vue FP avec souris capturée : les événements SOURIS appartiennent au
        // jeu — on ne les donne PAS à egui (le curseur invisible resterait
        // "au-dessus" d'un bouton d'UI et avalerait les clics de construction).
        let fp_owns_mouse = self.pointer_locked && self.sim.is_some();
        let is_mouse_event = matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::CursorLeft { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
        );
        let response = if fp_owns_mouse && is_mouse_event {
            egui_winit::EventResponse::default()
        } else {
            self.egui_winit.on_window_event(&self.window, &event)
        };
        self.egui_consumed_mouse = response.consumed;

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
                self.camera.aspect = self.renderer.width as f32 / self.renderer.height as f32;
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Position absolue : uniquement pour la visée UI / rayons.
                // La ROTATION caméra vient du raw input (handle_device_event) :
                // sur Windows, Locked pince le curseur dans un rect 1x1 px,
                // les deltas absolus y sont donc toujours nuls.
                self.cursor = (position.x as f32, position.y as f32);
                self.prev_cursor = self.cursor;
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                self.handle_mouse(btn_state, button);
            }
            WindowEvent::MouseWheel { delta: _, .. } => {
                // (premier personne : la molette est libre pour l'UI)
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    // HANGAR-OS ouvert : le clavier appartient à l'ordinateur
                    // (saisie libre dans le terminal). Seul Échap ferme.
                    if self.computer.open {
                        if matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {
                            self.close_computer();
                        }
                        return;
                    }
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            if self.pointer_locked {
                                // Echap : libérer la souris (accès menus/UI).
                                self.unlock_pointer();
                            }
                            self.build_kind = None;
                            self.demolish_mode = false;
                            self.selected = None;
                            self.show_help = false;
                        }
                        Key::Named(NamedKey::Space) => {
                            if !self.egui_consumed_mouse {
                                self.paused = !self.paused;
                            }
                        }
                        _ if event.physical_key == winit::keyboard::KeyCode::ShiftLeft
                            || event.physical_key == winit::keyboard::KeyCode::ShiftRight =>
                        {
                            self.keys.insert("shift".into());
                        }
                        Key::Character(c) => {
                            let c = c.as_str().to_ascii_lowercase();
                            match c.as_str() {
                                "r" => self.belt_dir = self.belt_dir.rotate_cw(),
                                "x" => self.demolish_mode = !self.demolish_mode,
                                "h" => self.show_help = !self.show_help,
                                "e" => self.try_use_computer(),
                                "f" => self.interact_floor(),
                                // 1-9 : sélection directe du stock à poser.
                                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                                    if let Ok(idx) = c.parse::<usize>() {
                                        if let Some(item) = ALL_SHOP_ITEMS.get(idx - 1) {
                                            self.build_kind = if self.build_kind == Some(*item) {
                                                None
                                            } else {
                                                Some(*item)
                                            };
                                            self.demolish_mode = false;
                                        }
                                    }
                                }
                                _ => {}
                            }
                            self.keys.insert(c);
                        }
                        _ => {}
                    }
                } else {
                    match &event.logical_key {
                        _ if event.physical_key == winit::keyboard::KeyCode::ShiftLeft
                            || event.physical_key == winit::keyboard::KeyCode::ShiftRight =>
                        {
                            self.keys.remove("shift");
                        }
                        Key::Character(c) => {
                            self.keys.remove(&c.as_str().to_ascii_lowercase());
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, state: ElementState, button: MouseButton) {
        // HANGAR-OS ouvert : tous les clics vont à l'UI.
        if self.computer.open {
            return;
        }
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                if self.egui_consumed_mouse {
                    return;
                }
                // En jeu, hors pointer lock : le clic (re)capture la souris
                // au lieu d'interagir (le crosshair pilote alors la visée).
                if !self.pointer_locked && self.sim.is_some() {
                    self.lock_pointer();
                    return;
                }
                // Rayon : crosshair (locked) ou curseur (déverrouillé).
                let (w, h) = (self.renderer.width as f32, self.renderer.height as f32);
                let ray = if self.pointer_locked {
                    self.camera.ray_center()
                } else {
                    self.camera.ray_at(self.cursor.0, self.cursor.1, w, h)
                };
                let Some((cx, cy)) = ray.pick_cell(h33_core::balance::HANGAR_HALF_SIZE) else {
                    self.selected = None;
                    return;
                };
                let Some(sim) = self.sim.as_mut() else { return };

                if self.demolish_mode {
                    if let Err(e) = sim.demolish_cell(cx, cy) {
                        self.push_toast(e, Severity::Warn);
                    }
                    return;
                }
                match self.build_kind {
                    Some(ShopItem::Belt) => {
                        let dir = self.belt_dir;
                        if let Err(e) = sim.place_belt(cx, cy, dir) {
                            self.push_toast(e, Severity::Warn);
                        }
                    }
                    Some(ShopItem::Machine(kind)) => {
                        let dir = self.belt_dir;
                        if let Err(e) = sim.place_machine(cx, cy, kind, dir) {
                            self.push_toast(e, Severity::Warn);
                        }
                    }
                    None => {
                        // Sélection d'une machine.
                        self.selected = sim.grid.get(&cell_key(cx, cy)).and_then(|c| match c {
                            CellContent::Machine(mi) => Some(*mi),
                            _ => None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    //  Ordinateur de gestion (HANGAR-OS)
    // ------------------------------------------------------------------

    /// E près de l'ordinateur : ouvre l'interface ; E/Échap dedans : ferme.
    fn try_use_computer(&mut self) {
        if self.computer.open {
            self.close_computer();
            return;
        }
        if self.near_computer && self.sim.is_some() {
            self.computer.open = true;
            self.build_kind = None;
            self.demolish_mode = false;
            self.unlock_pointer();
            if !self.computer.greeted {
                self.computer.greeted = true;
                // Première session : la Boutique est déjà ouverte sur le bureau.
                self.computer.apps[0] = true;
                self.computer.log.push("[info] nouvelle session — bienvenue au HANGAR 33.".into());
            }
        }
    }

    fn close_computer(&mut self) {
        self.computer.open = false;
        if self.sim.is_some() && !self.smoke_test {
            self.lock_pointer();
        }
    }

    // ------------------------------------------------------------------
    //  Raw input : le SEUL flux fiable pour le mouse look en pointer lock.
    //  Sur Windows, Locked = clip 1x1 px -> CursorMoved ne bouge plus ;
    //  seul DeviceEvent::MouseMotion donne les vrais deltas relatifs.
    // ------------------------------------------------------------------

    pub fn handle_device_event(&mut self, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            // Rotation uniquement quand la vue FP capture la souris.
            if self.pointer_locked && self.sim.is_some() && !self.computer.open {
                const SENS: f32 = 0.0023;
                self.camera.yaw += delta.0 as f32 * SENS;
                self.camera.pitch = (self.camera.pitch - delta.1 as f32 * SENS).clamp(-1.35, 1.35);
            }
        }
    }

    // ------------------------------------------------------------------
    //  Colis au sol (dock de palettes) : interaction contextuelle touche F
    //  — remplace l'ancien panneau latéral "Zone de palettes".
    // ------------------------------------------------------------------

    /// Position MONDE du slot de palette i (même formule que le rendu).
    fn floor_slot_pos(i: usize) -> Vec3 {
        let n = h33_core::balance::FLOOR_PACKAGE_LIMIT as f32;
        let x = i as f32 - (n - 1.0) * 0.5;
        Vec3::new(x, 0.8, h33_core::balance::DOCK_ZONE_Z)
    }

    fn nearest_floor_package(&self) -> Option<usize> {
        let sim = self.sim.as_ref()?;
        let mut best: Option<(f32, usize)> = None;
        for i in 0..sim.floor.len() {
            let d = self.camera.pos.distance(Self::floor_slot_pos(i));
            if d <= 3.5 && best.map_or(true, |(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// Texte du prompt contextuel (calculé chaque frame pour l'UI).
    fn floor_prompt_text(&self) -> Option<String> {
        if self.computer.open {
            return None;
        }
        let sim = self.sim.as_ref()?;
        let pi = self.nearest_floor_package()?;
        let shift = self.keys.contains("shift");
        Some(match (&sim.floor[pi].items, shift) {
            (None, false) => "[F] Ouvrir le colis   ·   [Shift+F] Revendre sans ouvrir (7 €)".into(),
            (None, true) => "[Shift+F] Revendre sans ouvrir (7 €)".into(),
            (Some(items), _) if items.iter().any(|k| *k == ItemKind::Bague) => {
                "[F] SÉCURISER LA BAGUE (victoire !)".into()
            }
            (Some(items), _) if items.is_empty() => "[F] Colis vide".into(),
            (Some(items), _) => format!(
                "[F] Vendre le contenu ({} objet{})",
                items.len(),
                if items.len() > 1 { "s" } else { "" }
            ),
        })
    }

    /// Touche F : action contextuelle sur le colis le plus proche du joueur.
    fn interact_floor(&mut self) {
        if self.computer.open {
            return;
        }
        let Some(pi) = self.nearest_floor_package() else { return };
        let Some(sim) = self.sim.as_mut() else { return };
        if sim.floor[pi].items.is_none() {
            if self.keys.contains("shift") {
                let price = h33_core::balance::PACKAGE_BLIND_SELL_EUR;
                if sim.manual_blind_sell(pi) {
                    self.push_toast(format!("Colis revendu sans ouvrir : +{price:.0} €"), Severity::Info);
                }
            } else if sim.manual_open(pi).is_some() {
                self.push_toast("Colis ouvert — rappuie sur F pour vendre le contenu", Severity::Info);
            }
            return;
        }
        // Ouvert : bague -> sécuriser (victoire) ; sinon tout vendre.
        if let Some(ii) = sim.floor[pi]
            .items
            .as_ref()
            .and_then(|v| v.iter().position(|k| *k == ItemKind::Bague))
        {
            sim.manual_secure_ring(pi, ii);
            return;
        }
        let before = sim.eco.money;
        // NB : vendre le dernier objet retire le colis de `floor`
        // (remove_floor_item) -> re-récupérer à chaque tour.
        while sim
            .floor
            .get(pi)
            .and_then(|p| p.items.as_ref())
            .is_some_and(|v| !v.is_empty())
        {
            if sim.manual_sell_item(pi, 0).is_none() {
                break;
            }
        }
        let gained = sim.eco.money - before;
        if gained > 0.0 {
            self.push_toast(format!("Contenu vendu : +{gained:.2} €"), Severity::Info);
        } else {
            self.push_toast("Colis vide", Severity::Info);
        }
    }

    // ------------------------------------------------------------------
    //  Pointer lock (vue première personne)
    // ------------------------------------------------------------------

    fn lock_pointer(&mut self) {
        // Sous Xvfb/headless le grab peut échouer : non bloquant (la vue
        // fonctionne au curseur déplacé, et le smoke-test n'en dépend pas).
        let grabbed = self
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            .or_else(|_| self.window.set_cursor_grab(winit::window::CursorGrabMode::Confined));
        let _ = grabbed;
        self.window.set_cursor_visible(false);
        self.pointer_locked = true;
    }

    fn unlock_pointer(&mut self) {
        let _ = self.window.set_cursor_grab(winit::window::CursorGrabMode::None);
        self.window.set_cursor_visible(true);
        self.pointer_locked = false;
        self.prev_cursor = self.cursor;
    }

    // ------------------------------------------------------------------
    //  Boucle de rendu
    // ------------------------------------------------------------------

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.frame += 1;
        let dt_real = self.last_frame.elapsed().as_secs_f32().min(0.25);
        self.last_frame = Instant::now();
        self.fps_avg = self.fps_avg * 0.95 + (1.0 / dt_real.max(1e-4)) * 0.05;

        // --- Déplacement première personne (ZQSD/WASD + sprint) ----------
        // (figé quand HANGAR-OS est ouvert : on est "assis au bureau")
        if !self.computer.open {
            self.update_fp_move(dt_real);
        } else {
            self.keys.clear();
        }

        // --- Proximité de l'ordinateur (prompt "[E]") ---------------------
        if self.sim.is_some() {
            let d = self.camera.pos.distance(COMPUTER_POS);
            self.near_computer = d <= h33_core::balance::COMPUTER_INTERACT_RADIUS_M;
        } else {
            self.near_computer = false;
        }

        // --- Prompt contextuel colis (touche F, dock de palettes) --------
        self.floor_prompt = if self.sim.is_some() { self.floor_prompt_text() } else { None };

        // --- Simulation à pas fixe (60 Hz) --------------------------------
        let mut sim_events: Vec<GameEvent> = Vec::new();
        if let Some(sim) = self.sim.as_mut() {
            if !self.paused {
                self.acc += dt_real;
                let mut steps = 0;
                while self.acc >= FIXED_DT && steps < MAX_STEPS_PER_FRAME {
                    sim.tick(FIXED_DT);
                    self.acc -= FIXED_DT;
                    steps += 1;
                }
                if steps == MAX_STEPS_PER_FRAME {
                    self.acc = 0.0; // anti-spirale de dette de temps
                }
            }
            sim_events.extend(sim.drain_events());
        }
        for ev in sim_events {
            match ev {
                GameEvent::Toast { text, severity } => self.push_toast(text, severity),
                GameEvent::RingSold { penalty_eur } => {
                    self.push_toast(format!("BAGUE VENDUE — {penalty_eur:.0} € en moins…"), Severity::Danger);
                }
                GameEvent::RingBurned => {
                    self.push_toast("LA BAGUE A BRÛLÉ. Elle rejoint le stock du fournisseur…", Severity::Danger);
                }
                GameEvent::RingSecured => {
                    self.push_toast("BAGUE SÉCURISÉE — VICTOIRE !", Severity::Info);
                }
                GameEvent::Blackout(b) => {
                    if b {
                        self.push_toast("BLACKOUT — vends à la main pour redémarrer", Severity::Danger);
                    } else {
                        self.push_toast("Courant rétabli", Severity::Info);
                    }
                }
                GameEvent::NoMoney => self.push_toast("Fonds insuffisants", Severity::Warn),
            }
        }
        if let Some(sim) = self.sim.as_ref() {
            let now = sim.elapsed_s;
            self.toasts.retain(|(_, _, expire)| *expire > now);
        }

        // --- Smoke test (script) ------------------------------------------
        if self.smoke_test {
            self.smoke_script(event_loop);
        }
        if let Some(max) = self.frames_limit {
            if self.frame >= max {
                event_loop.exit();
                return;
            }
        }

        // --- Cellule survolée (ghost) --------------------------------------
        {
            let (w, h) = (self.renderer.width as f32, self.renderer.height as f32);
            // Pointer lock : vise au crosshair ; sinon au curseur libre.
            let ray = if self.pointer_locked {
                self.camera.ray_center()
            } else {
                self.camera.ray_at(self.cursor.0, self.cursor.1, w, h)
            };
            self.hovered_cell = ray.pick_cell(h33_core::balance::HANGAR_HALF_SIZE);
        }

        // --- egui -----------------------------------------------------------
        let raw = self.egui_winit.take_egui_input(&self.window);
        let full = {
            // Split des emprunts : le closure UI manipule tout en &mut.
            let GameState {
                sim, camera, build_kind, belt_dir, demolish_mode, selected, toasts,
                paused, show_help, menu_packages, seed_fp, fps_avg, frame,
                start_request, back_to_menu, computer, near_computer, floor_prompt, ..
            } = self;
            let mut borrow = ui::UiBorrow {
                sim,
                camera,
                build_kind,
                belt_dir,
                demolish_mode,
                selected,
                toasts,
                paused,
                show_help,
                menu_packages,
                seed_fp,
                fps: *fps_avg,
                frame: *frame,
                start_request,
                back_to_menu,
                computer,
                near_computer: *near_computer,
                floor_prompt: floor_prompt.clone(),
            };
            self.egui_ctx.run(raw, move |ctx| ui::draw(ctx, &mut borrow))
        };
        self.egui_winit.handle_platform_output(&self.window, full.platform_output);

        // Actions UI différées (démarrage / retour menu / fermeture session).
        if let Some(total) = self.start_request.take() {
            self.start_game(total);
        }
        if self.back_to_menu {
            self.back_to_menu = false;
            self.sim = None;
        }
        if self.computer.close_request {
            self.computer.close_request = false;
            self.close_computer();
        }

        // --- Peinture ---------------------------------------------------------
        let frame_tex = match self.renderer.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.renderer.force_reconfigure();
                return;
            }
            Err(_) => return,
        };
        let view = frame_tex.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let cam_uniform = CameraUniform::from_mat(self.camera.view_proj_mat(), self.camera.pos);
        // Données de scène : instances cubes (procédural) + instances GLB.
        let (cube_instances, batch_data) = self.build_scene_data();
        let batches = [
            h33_render::renderer::MeshBatch { mesh: &self.assets.shell, instances: &batch_data.shell },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[0], instances: &batch_data.unboxer },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[1], instances: &batch_data.seller },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[2], instances: &batch_data.incinerator },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[3], instances: &batch_data.splitter },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[4], instances: &batch_data.scanner },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[5], instances: &batch_data.autobuyer },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[6], instances: &batch_data.robotarm },
            h33_render::renderer::MeshBatch { mesh: &self.assets.machines[7], instances: &batch_data.generator },
            h33_render::renderer::MeshBatch { mesh: &self.assets.package, instances: &batch_data.package },
            h33_render::renderer::MeshBatch { mesh: &self.assets.computer, instances: &batch_data.computer },
            h33_render::renderer::MeshBatch { mesh: &self.assets.open_crate, instances: &batch_data.open_crate },
            h33_render::renderer::MeshBatch { mesh: &self.assets.prism, instances: &batch_data.prism },
            h33_render::renderer::MeshBatch { mesh: &self.assets.sphere, instances: &batch_data.sphere },
            h33_render::renderer::MeshBatch { mesh: &self.assets.torus, instances: &batch_data.torus },
        ];

        // Peinture : scene + egui sur le même encoder.
        let SurfaceRenderer { gpu, scene, config, depth, .. } = &mut self.renderer;
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        scene.draw(&gpu.device, &gpu.queue, &mut encoder, &view, depth, &cam_uniform, &cube_instances, &batches);

        // egui par-dessus (Load : on garde la scène déjà dessinée).
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [config.width, config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };
        for (id, delta) in &full.textures_delta.set {
            self.egui_renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
        // Tessellation (egui 0.31 : FullOutput.shapes -> primitives).
        let paint_jobs = self
            .egui_ctx
            .tessellate(full.shapes, full.pixels_per_point);
        let upload_cmds =
            self.egui_renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &paint_jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui.pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime(); // requis par egui-wgpu 0.31 (RenderPass<'static>)
            self.egui_renderer.render(&mut pass, &paint_jobs, &screen);
        }
        for id in &full.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        // Les uploads egui doivent s'exécuter AVANT la passe egui.
        let mut cmds = upload_cmds;

        // Capture de la frame réelle (scène + egui) si demandée (smoke-test).
        let mut capture: Option<(wgpu::Buffer, u32, u32)> = None;
        if self.capture_request.take().is_some() {
            let (w, h) = (config.width, config.height);
            let bytes_per_row = (w * 4).div_ceil(256) * 256;
            let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame.capture"),
                size: (bytes_per_row * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                frame_tex.texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buf,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            capture = Some((buf, w, h));
        }

        cmds.push(encoder.finish());
        gpu.queue.submit(cmds);
        frame_tex.present();

        // Readback après submit (le poll attend la fin du GPU).
        if let Some((buf, w, h)) = capture {
            let (tx, rx) = std::sync::mpsc::channel();
            buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            let _ = gpu.device.poll(wgpu::Maintain::Wait);
            if rx.recv().map_or(true, |r| r.is_err()) {
                eprintln!("[h33] capture: mapping échoué");
                return;
            }
            let data = buf.slice(..).get_mapped_range().to_vec();
            buf.unmap();
            let row_bytes = (w * 4) as usize;
            let stride = (w * 4).div_ceil(256) * 256;
            let mut pixels = Vec::with_capacity(row_bytes * h as usize);
            for y in 0..h as usize {
                let start = y * stride as usize;
                pixels.extend_from_slice(&data[start..start + row_bytes]);
            }
            // La swapchain sRGB peut être Bgra8 : normalise en RGBA.
            let fmt = self.renderer.scene.color_format();
            if fmt == wgpu::TextureFormat::Bgra8UnormSrgb {
                for px in pixels.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
            }
            if let Some(path) = &self.last_capture_path {
                match h33_render::renderer::save_png(path, w, h, &pixels) {
                    Ok(()) => println!("[smoke] screenshot (frame réelle, avec UI): {}", path.display()),
                    Err(e) => eprintln!("[smoke] PNG échoué: {e}"),
                }
            }
            if self.smoke_test && self.smoke_step >= 7 {
                println!("[smoke] OK — arrêt propre (exit 0)");
                event_loop.exit();
            }
        }
    }

    /// Déplacement première personne : ZQSD/WASD (physique — AZERTY et
    /// QWERTY marchent), Shift = sprint, borné aux murs du hangar.
    fn update_fp_move(&mut self, dt: f32) {
        let mut move_x = 0.0f32;
        let mut move_z = 0.0f32;
        for k in &self.keys {
            match k.as_str() {
                "w" | "z" => move_x += 1.0, // avant
                "s" => move_x -= 1.0,       // arrière
                "a" | "q" => move_z -= 1.0, // gauche
                "d" => move_z += 1.0,       // droite
                _ => {}
            }
        }
        if move_x == 0.0 && move_z == 0.0 {
            return;
        }
        let sprint = self.keys.contains("shift");
        let speed = if sprint { 9.0 } else { 4.6 };
        let fwd = self.camera.forward();
        let fwd = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
        let right = self.camera.right();
        let delta = (fwd * move_x + right * move_z).normalize_or_zero() * speed * dt;
        self.camera.pos += delta;
        // Bornes intérieures (murs à ±33/±24 — marge ~1,5 m ; la zone de
        // palettes est DANS le hangar, contre le mur sud).
        self.camera.pos.x = self.camera.pos.x.clamp(-31.0, 31.0);
        self.camera.pos.z = self.camera.pos.z.clamp(-22.5, 22.5);
        self.camera.pos.y = FpCamera::EYE_HEIGHT;
    }

    // ------------------------------------------------------------------
    //  Instances 3D (sim -> GPU) : cubes procéduraux + lots GLB
    // ------------------------------------------------------------------

    fn build_scene_data(&self) -> (Vec<InstanceRaw>, SceneBatchData) {
        let mut cubes = Vec::with_capacity(2048);
        let mut glb = SceneBatchData {
            shell: Vec::new(),
            unboxer: Vec::new(),
            seller: Vec::new(),
            incinerator: Vec::new(),
            splitter: Vec::new(),
            scanner: Vec::new(),
            autobuyer: Vec::new(),
            robotarm: Vec::new(),
            generator: Vec::new(),
            package: Vec::new(),
            computer: Vec::new(),
            open_crate: Vec::new(),
            prism: Vec::new(),
            sphere: Vec::new(),
            torus: Vec::new(),
        };

        // Enveloppe du hangar : une seule instance identité.
        glb.shell.push(InstanceRaw::flat(
            glam::Vec3::ZERO,
            glam::Vec3::ONE,
            [1.0, 1.0, 1.0, 1.0],
        ));

        // Temps de sim (0 hors partie) — pulsations diverses.
        let t = self.sim.as_ref().map_or(0.0, |s| s.elapsed_s as f32);

        // Ordinateur de gestion : face au hangar (regarde -Z -> yaw = PI).
        // L'écran clignote doucement dès qu'une livraison est en cours.
        let deliveries_pending = self.sim.as_ref().map_or(0, |s| s.deliveries.len());
        let screen_glow = if deliveries_pending > 0 {
            1.0 + 0.5 * (t * 3.0).sin()
        } else {
            1.0
        };
        glb.computer.push(InstanceRaw::new(
            COMPUTER_POS,
            std::f32::consts::PI,
            glam::Vec3::ONE,
            [screen_glow, screen_glow, screen_glow, 1.0],
        ));

        let Some(sim) = self.sim.as_ref() else { return (cubes, glb) };

        // Dock de palettes (zone de livraison, DANS le hangar au sud).
        let dock_z = h33_core::balance::DOCK_ZONE_Z;
        let n_slots = h33_core::balance::FLOOR_PACKAGE_LIMIT as f32;
        for i in 0..h33_core::balance::FLOOR_PACKAGE_LIMIT {
            let x = i as f32 - (n_slots - 1.0) * 0.5;
            cubes.push(InstanceRaw::flat(
                glam::Vec3::new(x, 0.05, dock_z),
                glam::Vec3::new(0.9, 0.1, 0.9),
                [0.35, 0.25, 0.16, 1.0],
            ));
        }
        for (i, pkg) in sim.floor.iter().enumerate() {
            let x = i as f32 - (n_slots - 1.0) * 0.5;
            match &pkg.items {
                None => {
                    // Colis fermé sur sa palette : vrai modèle GLB.
                    glb.package.push(InstanceRaw::new(
                        glam::Vec3::new(x, 0.1, dock_z),
                        (i as f32 * 1.7).sin() * 0.5,
                        glam::Vec3::ONE,
                        [1.0, 1.0, 1.0, 1.0],
                    ));
                }
                Some(items) => {
                    // Colis ouvert : VRAI carton ouvert (parois + rabats
                    // rabattus) + objets formés (cylindre/sphère/tore/cube)
                    // qui sortent du carton avec une animation "pop".
                    let age = pkg.opened_at_s.map_or(10.0, |o| (t - o as f32).max(0.0));
                    let pop = ease_out_back((age / 0.55).min(1.0));
                    let hop_k = (age / 0.45).min(1.0);
                    let hop = (hop_k * std::f32::consts::PI).sin() * 0.08 * (1.0 - hop_k);
                    glb.open_crate.push(InstanceRaw::new(
                        glam::Vec3::new(x, 0.106 + hop, dock_z),
                        (i as f32 * 1.7).sin() * 0.5,
                        glam::Vec3::splat(0.92),
                        [0.85, 0.66, 0.44, 1.0], // kraft
                    ));
                    // LA BAGUE dans un carton ouvert : pilier de lumière doré.
                    if items.iter().any(|k| *k == ItemKind::Bague) {
                        let pulse = 0.7 + 0.3 * (t * 4.0).sin();
                        cubes.push(InstanceRaw::flat(
                            glam::Vec3::new(x, 1.6, dock_z),
                            glam::Vec3::new(0.06, 3.0, 0.06),
                            [2.2 * pulse, 1.8 * pulse, 0.5, 1.0],
                        ));
                    }
                    for (j, kind) in items.iter().take(6).enumerate() {
                        let jx = (j % 3) as f32 * 0.26 - 0.26;
                        let jz = (j / 3) as f32 * 0.26 - 0.13;
                        // Les objets grandissent DANS le carton (pop) : le
                        // scale quasi nul au départ évite de traverser le fond.
                        let pos = glam::Vec3::new(x + jx, 0.106, dock_z + jz);
                        let yaw = i as f32 * 0.7 + j as f32 * 1.3;
                        push_item(&mut cubes, &mut glb, *kind, pos, yaw, pop.max(0.05), t);
                    }
                }
            }
        }

        // Marqueur "LIVRAISON" : losange doré pulsant qui flotte au-dessus de
        // la zone de palettes tant qu'un colis y attend (impossible à rater,
        // même depuis l'autre bout du hangar).
        if !sim.floor.is_empty() {
            let bob = (t * 2.4).sin() * 0.25;
            let spin = t * 1.2;
            let glow = 0.75 + 0.25 * (t * 3.0).sin();
            cubes.push(InstanceRaw::new(
                glam::Vec3::new(0.0, 2.8 + bob, dock_z),
                spin,
                glam::Vec3::splat(0.45),
                [2.0 * glow, 1.6 * glow, 0.4, 1.0],
            ));
        }

        // Grille : tapis + machines.
        for (key, content) in &sim.grid {
            let (cx, cy) = h33_core::sim::cell_xy(*key);
            let (wx, wz) = (cx as f32, cy as f32);
            match content {
                CellContent::Belt(bi) => {
                    let belt = &sim.belts[*bi];
                    if belt.removed {
                        continue;
                    }
                    let (dx, dy) = belt.dir.delta();
                    cubes.push(InstanceRaw::flat(
                        glam::Vec3::new(wx, 0.06, wz),
                        glam::Vec3::new(0.94, 0.12, 0.94),
                        [0.22, 0.23, 0.26, 1.0],
                    ));
                    // Chevron de direction (bord sortant).
                    cubes.push(InstanceRaw::flat(
                        glam::Vec3::new(wx + dx as f32 * 0.38, 0.13, wz + dy as f32 * 0.38),
                        if dx != 0 {
                            glam::Vec3::new(0.16, 0.03, 0.06)
                        } else {
                            glam::Vec3::new(0.06, 0.03, 0.16)
                        },
                        [0.45, 0.75, 0.9, 1.0],
                    ));
                    // Objet porté : colis fermé = vrai modèle GLB, objets
                    // dépaquetés = formes distinctes (cylindre/sphère/tore...).
                    if let Some(item) = &belt.item {
                        let px = wx + dx as f32 * (item.progress - 0.5);
                        let pz = wz + dy as f32 * (item.progress - 0.5);
                        if item.kind == ItemKind::ColisFerme {
                            glb.package.push(InstanceRaw::new(
                                glam::Vec3::new(px, 0.12, pz),
                                ((wx * 7.0 + wz * 13.0) * 0.35).sin() * 0.6,
                                glam::Vec3::ONE,
                                [1.0, 1.0, 1.0, 1.0],
                            ));
                        } else {
                            let yaw = (wx * 3.1 + wz * 5.7) * 0.8;
                            push_item(
                                &mut cubes, &mut glb, item.kind,
                                glam::Vec3::new(px, 0.12, pz), yaw, 1.0, t,
                            );
                        }
                    }
                }
                CellContent::Machine(mi) => {
                    let m = &sim.machines[*mi];
                    if m.removed {
                        continue;
                    }
                    // Modèle GLB orienté : le modèle regarde +Z, la machine
                    // regarde sa Dir -> yaw = atan2(dx, dz).
                    let (dx, dy) = m.dir.delta();
                    let yaw = (dx as f32).atan2(dy as f32);
                    let inst = InstanceRaw::new(
                        glam::Vec3::new(wx, 0.0, wz),
                        yaw,
                        glam::Vec3::ONE,
                        if m.busy_with.is_some() { [1.3, 1.3, 1.3, 1.0] } else { [1.0, 1.0, 1.0, 1.0] },
                    );
                    match m.kind {
                        MachineKind::Unpacker => glb.unboxer.push(inst),
                        MachineKind::Seller => glb.seller.push(inst),
                        MachineKind::Incinerator => glb.incinerator.push(inst),
                        MachineKind::Splitter => glb.splitter.push(inst),
                        MachineKind::RingScanner => glb.scanner.push(inst),
                        MachineKind::AutoBuyer => glb.autobuyer.push(inst),
                        MachineKind::RobotArm => glb.robotarm.push(inst),
                        MachineKind::Generator => glb.generator.push(inst),
                    }
                    // Marqueur de sortie (côté dir).
                    cubes.push(InstanceRaw::flat(
                        glam::Vec3::new(wx + dx as f32 * 0.55, 0.14, wz + dy as f32 * 0.55),
                        glam::Vec3::new(0.22, 0.1, 0.22),
                        brighten(machine_color(m.kind), 1.8),
                    ));
                }
            }
        }

        // Ghost de placement.
        if let (Some(kind), Some((cx, cy))) = (self.build_kind, self.hovered_cell) {
            let mut c = shop_color(kind);
            c[0] *= 1.3;
            c[1] *= 1.3;
            c[2] *= 1.3;
            c[3] = 0.85;
            cubes.push(InstanceRaw::flat(
                glam::Vec3::new(cx as f32, 0.3, cy as f32),
                glam::Vec3::splat(0.7),
                c,
            ));
        }
        // Sélection : socle blanc.
        if let Some(mi) = self.selected {
            if let Some(key) = sim.machine_cell.get(mi) {
                let (cx, cy) = h33_core::sim::cell_xy(*key);
                cubes.push(InstanceRaw::flat(
                    glam::Vec3::new(cx as f32, 0.045, cy as f32),
                    glam::Vec3::new(1.1, 0.04, 1.1),
                    [1.0, 1.0, 1.0, 1.0],
                ));
            }
        }
        // Highlight démolition.
        if self.demolish_mode {
            if let Some((cx, cy)) = self.hovered_cell {
                cubes.push(InstanceRaw::flat(
                    glam::Vec3::new(cx as f32, 0.045, cy as f32),
                    glam::Vec3::new(1.0, 0.04, 1.0),
                    [0.95, 0.2, 0.2, 1.0],
                ));
            }
        }
        (cubes, glb)
    }

    // ------------------------------------------------------------------
    //  Smoke-test scripté (CI + lavapipe)
    // ------------------------------------------------------------------

    fn smoke_script(&mut self, _event_loop: &ActiveEventLoop) {
        if self.sim.is_none() {
            self.start_game(1_000_000);
            self.smoke_step = 1;
            return;
        }
        if self.smoke_step == 1 && self.frame > 10 {
            let Some(sim) = self.sim.as_mut() else { return };
            // Boucle manuelle complète : achat -> ouverture -> vente/brûlage.
            for _ in 0..3 {
                sim.manual_buy_package();
            }
            while !sim.floor.is_empty() {
                let pi = sim.floor.len() - 1;
                sim.manual_open(pi);
                while sim
                    .floor
                    .get(pi)
                    .and_then(|p| p.items.as_ref())
                    .is_some_and(|v| !v.is_empty())
                {
                    let is_carton = sim
                        .floor
                        .get(pi)
                        .and_then(|p| p.items.as_ref())
                        .map_or(false, |v| v[0] == ItemKind::CartonVide);
                    if is_carton {
                        sim.manual_burn_item(pi, 0);
                    } else {
                        sim.manual_sell_item(pi, 0);
                    }
                }
            }
            println!(
                "[smoke] boucle manuelle OK — colis achetés: {}, argent: {:.2} €",
                sim.eco.packages_bought, sim.eco.money
            );
            // Interaction F (dock de palettes) : acheter, ouvrir (F), vendre
            // tout (F) — la boucle "en monde" qui remplace le panneau latéral.
            let saved = (self.camera.pos, self.camera.yaw);
            self.camera.pos = Vec3::new(-11.5, FpCamera::EYE_HEIGHT, 22.3);
            self.camera.yaw = std::f32::consts::FRAC_PI_2; // regarde le dock (+Z, désormais DANS le hangar)
            assert!(self.sim.as_mut().unwrap().manual_buy_package(), "smoke: achat colis dock");
            self.interact_floor(); // F : ouvre
            assert!(
                self.sim
                    .as_ref()
                    .is_some_and(|s| s.floor[0].items.as_ref().is_some_and(|v| !v.is_empty())),
                "smoke: F n'a pas ouvert le colis"
            );
            self.interact_floor(); // F : vend tout le contenu
            assert!(
                self.sim.as_ref().is_some_and(|s| s.floor.is_empty()
                    || s.floor[0].items.as_ref().is_some_and(|v| v.is_empty())),
                "smoke: F n'a pas vendu le contenu"
            );
            println!("[smoke] touche F (dock) OK — ouvrir + vendre via interaction monde");
            self.camera.pos = saved.0;
            self.camera.yaw = saved.1;
            self.smoke_step = 2;
        } else if self.smoke_step == 2 && self.frame > 15 {
            let Some(sim) = self.sim.as_mut() else { return };
            // Commande de la ligne à l'ORDINATEUR (boutique HANGAR-OS),
            // puis forçage des livraisons (CI), puis placement du stock.
            sim.eco.earn(3000.0);
            for (item, qty) in [
                (ShopItem::Machine(MachineKind::AutoBuyer), 1),
                (ShopItem::Belt, 5),
                (ShopItem::Machine(MachineKind::Unpacker), 1),
                (ShopItem::Machine(MachineKind::Seller), 1),
                (ShopItem::Machine(MachineKind::Generator), 1),
                (ShopItem::Machine(MachineKind::RobotArm), 1),
                (ShopItem::Machine(MachineKind::Incinerator), 1),
            ] {
                sim.order(item, qty).expect("commande smoke-test");
            }
            sim.order_packages(5).expect("commande colis smoke-test");
            sim.debug_force_deliveries();
            sim.tick(1.0 / 60.0); // fait arriver les livraisons (à l'instant)
            println!(
                "[smoke] commandes passées — argent: {:.2} €, stock: {:?}",
                sim.eco.money,
                sim.stock.iter().sum::<u32>()
            );
            // Ligne automatique traversant le BRAS ROBOT :
            // AutoBuyer -> tapis -> tapis -> Dépaqueteur -> tapis -> tapis ->
            // BrasRobot -> tapis -> Guichet. Générateur à côté (kW gratuits).
            let _ = sim.place_machine(-2, 0, MachineKind::AutoBuyer, Dir::East);
            let _ = sim.place_belt(-1, 0, Dir::East);
            let _ = sim.place_belt(0, 0, Dir::East);
            let _ = sim.place_machine(1, 0, MachineKind::Unpacker, Dir::East);
            let _ = sim.place_belt(2, 0, Dir::East);
            let _ = sim.place_belt(3, 0, Dir::East);
            let _ = sim.place_machine(4, 0, MachineKind::RobotArm, Dir::East);
            let _ = sim.place_belt(5, 0, Dir::East);
            let _ = sim.place_machine(6, 0, MachineKind::Seller, Dir::East);
            let _ = sim.place_machine(-2, 3, MachineKind::Incinerator, Dir::North);
            let _ = sim.place_machine(2, 2, MachineKind::Generator, Dir::South);
            println!(
                "[smoke] ligne construite (avec bras robot + générateur) — stock restant: {:?}",
                sim.stock
            );
            self.smoke_step = 3;
        } else if self.smoke_step == 3 {
            // Diagnostic périodique (toutes les 2 s de sim) : compteurs.
            if self.frame % 120 == 0 {
                if let Some(sim) = self.sim.as_ref() {
                    let ub = sim.machines.iter().find(|m| m.kind == MachineKind::Unpacker).map(|m| (m.processed, m.busy_with.is_some(), m.out_queue.iter().flatten().count()));
                    let ab = sim.machines.iter().find(|m| m.kind == MachineKind::AutoBuyer).map(|m| (m.processed, m.cooldown_s));
                    let ra = sim.machines.iter().find(|m| m.kind == MachineKind::RobotArm).map(|m| (m.processed, m.busy_with.is_some(), m.out_queue.iter().flatten().count()));
                    let sd = sim.machines.iter().find(|m| m.kind == MachineKind::Seller).map(|m| (m.processed, m.busy_with.is_some()));
                    let belt_items: Vec<Option<f32>> = sim.belts.iter().map(|b| b.item.as_ref().map(|i| i.progress)).collect();
                    println!("[smoke dbg] t={:.1} autobuyer={ab:?} unpacker={ub:?} arm={ra:?} seller={sd:?} belts={belt_items:?}", sim.elapsed_s);
                }
            }
            // On attend que TOUTE la chaîne ait produit : le colis ouvert
            // met ~2 s à atteindre le bras robot, puis ~1,5 s le guichet
            // (t≈9 s de sim au total).
            let ready = self.sim.as_ref().map_or(false, |s| {
                s.machines.iter().any(|m| m.kind == MachineKind::Unpacker && m.processed >= 1)
                    && s.machines.iter().any(|m| m.kind == MachineKind::RobotArm && m.processed >= 1)
                    && s.machines.iter().any(|m| m.kind == MachineKind::Seller && m.processed >= 1)
            });
            if ready {
                let Some(sim) = self.sim.as_ref() else { return };
                let unpacked = sim.machines.iter().find(|m| m.kind == MachineKind::Unpacker).map(|m| m.processed).unwrap_or(0);
                let sold = sim.machines.iter().find(|m| m.kind == MachineKind::Seller).map(|m| m.processed).unwrap_or(0);
                let roboted = sim.machines.iter().find(|m| m.kind == MachineKind::RobotArm).map(|m| m.processed).unwrap_or(0);
                let (_, free) = sim.power_summary();
                println!(
                    "[smoke] chaîne productive à {:.1} s — dépaquetés: {}, vendus: {}, bras robot: {}, kW gratuits (générateur): {:.1}, blackout: {}",
                    sim.elapsed_s, unpacked, sold, roboted, free, sim.blackout
                );
                assert!(unpacked >= 1, "le dépaqueteur n'a rien ouvert");
                assert!(roboted >= 1, "le bras robot n'a rien manipulé");
                assert!(sold >= 1, "le guichet n'a rien vendu");
                assert!(free >= h33_core::balance::GENERATOR_FREE_KW, "le générateur ne produit pas");
                self.smoke_step = 4;
            }
        } else if self.smoke_step == 4 {
            // Vérifie la zone de palettes (colis commandés à l'ordinateur).
            let Some(sim) = self.sim.as_ref() else { return };
            assert!(!sim.floor.is_empty(), "les colis commandés ne sont pas arrivés");
            println!(
                "[smoke] ordinateur OK — {} colis livrés sur les palettes, {} livraison(s) en cours",
                sim.floor.len(),
                sim.deliveries.len()
            );
            // Capture 1/2 : vue FP de la ligne (HANGAR-OS fermé).
            self.last_capture_path = self.screenshot.clone();
            self.capture_request = self.screenshot.clone();
            self.smoke_step = 5;
        } else if self.smoke_step == 5 {
            // Ouvre HANGAR-OS (onglet Terminal) et tape de VRAIES commandes :
            // elles agissent réellement sur la sim (commande de colis payée).
            self.computer.open = true;
            self.computer.apps[2] = true; // app Terminal ouverte sur le faux bureau
            self.near_computer = true;
            for c in ["help", "status", "stock", "order colis 5"] {
                let mut io = ui::TermIo {
                    sim: &mut self.sim,
                    log: &mut self.computer.log,
                    toasts: &mut self.toasts,
                    package_qty: self.computer.package_qty,
                };
                ui::terminal_exec(c.to_string(), &mut io);
            }
            // La commande du terminal a débité la sim.
            let sim = self.sim.as_ref().expect("sim active");
            assert!(
                sim.deliveries.iter().any(|d| matches!(d.item, h33_core::shop::DeliveryKind::Packages(5))),
                "la commande 'order colis 5' du terminal n'a pas agi sur la sim"
            );
            println!("[smoke] terminal HANGAR-OS : 4 commandes tapées, commande colis réelle confirmée");
            self.smoke_step = 6;
        } else if self.smoke_step == 6 {
            // Capture 2/2 : l'écran du terminal (fenêtre egui HANGAR-OS).
            self.last_capture_path = self.terminal_screenshot.clone();
            self.capture_request = self.terminal_screenshot.clone();
            self.smoke_step = 7;
        }
    }
}

// ======================================================================
//  Couleurs
// ======================================================================

// ======================================================================
//  Formes des objets dépaquetés (v0.3.10) — le déballage se lit d'un
//  coup d'œil : chaque objet a SA forme (cylindre, sphère, tore, cube).
// ======================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemShape {
    Cube,
    Prism,
    Sphere,
    Torus,
    Crate,
}

/// Forme + échelle de base de chaque objet (mesh unitaire -> scale).
fn item_shape(kind: ItemKind) -> (ItemShape, glam::Vec3) {
    use ItemKind::*;
    match kind {
        CartonVide => (ItemShape::Crate, glam::Vec3::splat(0.3)),
        Chaussette => (ItemShape::Sphere, glam::Vec3::new(0.26, 0.13, 0.2)),
        Journal => (ItemShape::Cube, glam::Vec3::new(0.3, 0.05, 0.22)),
        Tournevis => (ItemShape::Prism, glam::Vec3::new(0.05, 0.32, 0.05)),
        Bougie => (ItemShape::Prism, glam::Vec3::new(0.14, 0.22, 0.14)),
        Montre => (ItemShape::Sphere, glam::Vec3::new(0.15, 0.07, 0.15)),
        ManetteRetro => (ItemShape::Cube, glam::Vec3::new(0.24, 0.09, 0.15)),
        Figurine => (ItemShape::Sphere, glam::Vec3::new(0.11, 0.28, 0.11)),
        JeuVideoRare => (ItemShape::Cube, glam::Vec3::new(0.22, 0.045, 0.16)),
        ConsoleRetro => (ItemShape::Cube, glam::Vec3::new(0.32, 0.1, 0.24)),
        VaseAncien => (ItemShape::Prism, glam::Vec3::new(0.18, 0.3, 0.18)),
        BijouFantaisie => (ItemShape::Sphere, glam::Vec3::splat(0.13)),
        // LA BAGUE : un anneau doré qui tourne sur lui-même.
        Bague => (ItemShape::Torus, glam::Vec3::splat(0.45)),
        ColisFerme => (ItemShape::Cube, glam::Vec3::splat(0.3)),
    }
}

/// Demi-hauteur de la forme à l'échelle finale (pour poser l'objet sur
/// une surface). Le carton a son origine au FOND : demi-hauteur nulle.
fn shape_half_height(shape: ItemShape, scale: glam::Vec3) -> f32 {
    match shape {
        ItemShape::Cube | ItemShape::Prism | ItemShape::Sphere => 0.5 * scale.y,
        ItemShape::Torus => 0.13 * scale.y, // petit rayon du tore
        ItemShape::Crate => 0.0,
    }
}

/// Pousse l'instance de l'objet dans le bon lot (cube procédural ou mesh
/// procédural), posé sur la surface `pos.y`.
fn push_item(
    cubes: &mut Vec<InstanceRaw>,
    glb: &mut SceneBatchData,
    kind: ItemKind,
    pos: glam::Vec3,
    yaw: f32,
    scale_mul: f32,
    t: f32,
) {
    let (shape, base_scale) = item_shape(kind);
    let scale = base_scale * scale_mul;
    let y = pos.y + shape_half_height(shape, scale);
    let inst = InstanceRaw::new(glam::Vec3::new(pos.x, y, pos.z), yaw, scale, item_color(kind, t));
    match shape {
        ItemShape::Cube => cubes.push(inst),
        ItemShape::Crate => glb.open_crate.push(inst),
        ItemShape::Prism => glb.prism.push(inst),
        ItemShape::Sphere => glb.sphere.push(inst),
        ItemShape::Torus => glb.torus.push(inst),
    }
}

/// Ease-out-back : arrive vite, dépasse un peu, se pose — l'effet "pop".
fn ease_out_back(x: f32) -> f32 {
    let c1 = 1.70158_f32;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

pub fn item_color(kind: ItemKind, t: f32) -> [f32; 4] {
    match kind {
        ItemKind::CartonVide => [0.82, 0.64, 0.42, 1.0],
        ItemKind::Chaussette => [0.95, 0.95, 1.0, 1.0],
        ItemKind::Journal => [0.8, 0.78, 0.6, 1.0],
        ItemKind::Tournevis => [0.9, 0.45, 0.25, 1.0],
        ItemKind::Bougie => [0.95, 0.75, 0.5, 1.0],
        ItemKind::Montre => [0.85, 0.87, 0.9, 1.0],
        ItemKind::ManetteRetro => [0.45, 0.55, 0.78, 1.0],
        ItemKind::Figurine => [0.95, 0.4, 0.55, 1.0],
        ItemKind::JeuVideoRare => [0.35, 0.85, 0.6, 1.0],
        ItemKind::ConsoleRetro => [0.65, 0.65, 0.72, 1.0],
        ItemKind::VaseAncien => [0.55, 0.78, 0.88, 1.0],
        ItemKind::BijouFantaisie => [1.0, 0.85, 0.35, 1.0],
        ItemKind::ColisFerme => [0.72, 0.55, 0.35, 1.0],
        // Pulsation dorée — impossible à rater sur un tapis.
        ItemKind::Bague => {
            let pulse = 1.0 + 0.35 * (t * 4.0).sin();
            [1.0 * pulse, 0.8 * pulse, 0.15, 1.0]
        }
    }
}

pub fn machine_color(kind: MachineKind) -> [f32; 4] {
    match kind {
        MachineKind::Unpacker => [0.85, 0.5, 0.15, 1.0],
        MachineKind::Seller => [0.2, 0.7, 0.35, 1.0],
        MachineKind::Incinerator => [0.55, 0.15, 0.1, 1.0],
        MachineKind::Splitter => [0.15, 0.6, 0.6, 1.0],
        MachineKind::RingScanner => [0.6, 0.3, 0.85, 1.0],
        MachineKind::AutoBuyer => [0.2, 0.45, 0.85, 1.0],
        MachineKind::RobotArm => [0.95, 0.65, 0.1, 1.0],
        MachineKind::Generator => [0.75, 0.2, 0.15, 1.0],
    }
}

/// Couleur de ghost pour un article de la boutique.
pub fn shop_color(item: ShopItem) -> [f32; 4] {
    match item {
        ShopItem::Belt => [0.45, 0.75, 0.9, 1.0],
        ShopItem::Machine(k) => machine_color(k),
    }
}

pub fn brighten(c: [f32; 4], f: f32) -> [f32; 4] {
    [c[0] * f, c[1] * f, c[2] * f, c[3]]
}
