//! Interface egui : menu de démarrage, HUD, barre de construction,
//! panneau palettes (déballage manuel), panneau machine, toasts, aide,
//! et l'ORDINATEUR de gestion (HANGAR-OS) : boutique, commandes de colis
//! et terminal (avec son code secret).
//! (Pas d'emoji : les fontes par défaut d'egui n'en ont pas tous.)

use egui::{Align, Color32, RichText};
use h33_core::balance;
use h33_core::economy::RingState;
use h33_core::machines::MachineKind;
use h33_core::shop::{DeliveryKind, ShopItem, ALL_SHOP_ITEMS};
use h33_core::sim::{Severity, Sim};
use h33_render::camera::FpCamera;

use crate::app::ComputerUi;

/// Emprunts splités de l'état — l'UI lit et agit directement.
pub struct UiBorrow<'a> {
    pub sim: &'a mut Option<Sim>,
    #[allow(dead_code)] // utilisable par les futurs widgets caméra
    pub camera: &'a mut FpCamera,
    pub build_kind: &'a mut Option<ShopItem>,
    pub belt_dir: &'a mut h33_core::grid::Dir,
    pub demolish_mode: &'a mut bool,
    pub selected: &'a mut Option<usize>,
    pub toasts: &'a mut Vec<(String, Severity, f64)>,
    pub paused: &'a mut bool,
    pub show_help: &'a mut bool,
    pub menu_packages: &'a mut f64,
    pub seed_fp: &'a str,
    pub fps: f32,
    #[allow(dead_code)]
    pub frame: u64,
    /// Demande de démarrage/nouvelle partie posée par l'UI (consommée par app).
    pub start_request: &'a mut Option<u64>,
    /// Demande de retour au menu (après victoire).
    pub back_to_menu: &'a mut bool,
    /// Ordinateur de gestion (HANGAR-OS).
    pub computer: &'a mut ComputerUi,
    /// Le joueur est à portée de l'ordinateur ?
    pub near_computer: bool,
    /// Prompt contextuel du colis le plus proche (touche F, dock).
    pub floor_prompt: Option<String>,
}

fn sev_color(s: Severity) -> Color32 {
    match s {
        Severity::Info => Color32::from_rgb(120, 200, 255),
        Severity::Warn => Color32::from_rgb(255, 190, 90),
        Severity::Danger => Color32::from_rgb(255, 90, 90),
    }
}

fn money_color() -> Color32 {
    Color32::from_rgb(140, 235, 160)
}

fn gold() -> Color32 {
    Color32::from_rgb(255, 200, 80)
}

fn terminal_green() -> Color32 {
    Color32::from_rgb(120, 255, 140)
}

fn fmt_eur(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{:.2} M€", v / 1e6)
    } else if v.abs() >= 10_000.0 {
        format!("{:.1} k€", v / 1e3)
    } else {
        format!("{v:.2} €")
    }
}

pub fn draw(ctx: &egui::Context, b: &mut UiBorrow) {
    let victory = b.sim.as_ref().is_some_and(|s| s.eco.ring_state == RingState::Secured);
    if b.sim.is_none() {
        draw_menu(ctx, b);
        return;
    }
    draw_top_bar(ctx, b);
    if b.computer.open {
        // BUREAU « HANGAR-OS 11 » : fond d'écran, icônes, fenêtres, taskbar.
        draw_computer(ctx, b);
        draw_toasts(ctx, b);
        return;
    }
    draw_hint(ctx, b);
    draw_selected_panel(ctx, b);
    draw_toasts(ctx, b);
    draw_help(ctx, b);
    draw_computer_prompt(ctx, b);
    draw_floor_prompt(ctx, b);
    if victory {
        // Split final : le panneau victoire couvre le centre.
        let UiBorrow { sim, back_to_menu, .. } = b;
        if let Some(sim) = sim.as_ref() {
            draw_victory(ctx, sim, back_to_menu);
        }
    }
}

// ======================================================================
//  Menu de démarrage
// ======================================================================

fn draw_menu(ctx: &egui::Context, b: &mut UiBorrow) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(12, 14, 18)))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.10);
                ui.heading(RichText::new("HANGAR 33").size(56.0).color(gold()).strong());
                ui.label(RichText::new("Déballe. Automatise. Trouve la bague.").size(18.0).weak());
                ui.add_space(26.0);

                let total = 10f64.powf(*b.menu_packages);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_width(560.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Nombre TOTAL de colis de la partie").strong());
                        ui.add_space(4.0);
                        ui.add(
                            egui::Slider::new(b.menu_packages, 6.0..=8.699)
                                .logarithmic(true)
                                .text("1 M ← → 500 M"),
                        );
                        ui.label(
                            RichText::new(format!("{} colis", h33_core::sim::format_count(total as u64)))
                                .size(24.0)
                                .color(gold()),
                        );
                        ui.label(
                            RichText::new(
                                "La bague est cachée dans UN de ces colis. Chaque achat retire le prochain colis de la file.",
                            )
                            .weak()
                            .small(),
                        );
                    });
                });

                ui.add_space(12.0);
                ui.label(RichText::new(format!("Empreinte seed TRNG : {}", b.seed_fp)).weak().small());
                ui.add_space(16.0);

                let start = egui::Button::new(RichText::new("COMMENCER").size(24.0).strong());
                if ui.add(start).clicked() {
                    *b.start_request = Some(total as u64);
                }
                ui.add_space(18.0);
                ui.label(
                    RichText::new(
                        "ZQSD : marcher · Souris : regarder · Shift : courir\n\
                         Va à l'ORDINATEUR (bureau sud-ouest) et appuie sur E : boutique, colis, terminal.\n\
                         1-9 : choisir le stock à poser · Clic : poser · F : colis au dock sud · R : rotation · X : pelle · Échap : souris libre · Espace : pause",
                    )
                    .weak()
                    .small(),
                );
            });
        });
}

// ======================================================================
//  HUD haut
// ======================================================================

fn draw_top_bar(ctx: &egui::Context, b: &mut UiBorrow) {
    let sim = match b.sim.as_ref() { Some(s) => s, None => return };
    egui::TopBottomPanel::top("hud").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("HANGAR 33").strong().color(gold()));
            ui.separator();
            ui.label(RichText::new(format!("{:.2} €", sim.eco.money)).color(money_color()).strong());
            ui.separator();
            let (demand, free) = sim.power_summary();
            ui.label(format!("ELEC {demand:.1} kW"));
            if free > 0.0 {
                ui.label(
                    RichText::new(format!("-{free:.1} kW gratuits"))
                        .color(Color32::from_rgb(255, 160, 90))
                        .small(),
                );
            }
            ui.separator();
            ui.label(format!("Colis restants : {}", h33_core::sim::format_count(sim.eco.packages_left)));
            ui.separator();
            if !sim.deliveries.is_empty() {
                // Compte à rebours de la PROCHAINE livraison (le joueur doit
                // toujours savoir quand sa commande arrive).
                let now = sim.elapsed_s;
                let next = sim.deliveries.iter().map(|d| d.eta_s).fold(f64::INFINITY, f64::min);
                let remaining = (next - now).max(0.0);
                ui.label(
                    RichText::new(format!("Livraison dans {remaining:.0} s ({} en route)", sim.deliveries.len()))
                        .color(sev_color(Severity::Info)),
                );
                ui.separator();
            }
            match sim.eco.ring_state {
                RingState::Secured => ui.label(RichText::new("BAGUE SÉCURISÉE").color(gold()).strong()),
                RingState::Hidden => {
                    let detail = sim.ring_status_text();
                    ui.label(RichText::new(format!("Bague : {detail}")).weak())
                }
            };
            ui.separator();
            if sim.blackout {
                ui.label(RichText::new("BLACKOUT").color(Color32::RED).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.0} FPS", b.fps)).weak().small());
                let pause_txt = if *b.paused { "Reprendre" } else { "Pause" };
                if ui.button(pause_txt).clicked() {
                    *b.paused = !*b.paused;
                }
                if ui.button("?").clicked() {
                    *b.show_help = !*b.show_help;
                }
            });
        });
    });
}

// ======================================================================
//  Aide clavier minimale (texte seul, sans boutons) + prompt colis (F)
// ======================================================================

/// Ligne d'aide discrète en bas d'écran : remplace la barre de
/// construction et le panneau latéral (scène de jeu propre).
fn draw_hint(ctx: &egui::Context, b: &UiBorrow) {
    let Some(sim) = b.sim.as_ref() else { return };
    egui::Area::new(egui::Id::new("hint"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -6.0])
        .show(ctx, |ui| {
            egui::Frame::group(ui.style())
                .fill(Color32::from_black_alpha(110))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        let mut line1 = String::new();
                        if *b.demolish_mode {
                            line1.push_str("PELLE — clic : démolir (50 % remboursés)");
                        } else if let Some(item) = *b.build_kind {
                            let stock = sim.stock[item.stock_index()];
                            if item == ShopItem::Belt {
                                line1.push_str(&format!(
                                    "POSER : {} ×{} [{}] — clic dans la scène",
                                    item.name(),
                                    stock,
                                    arrow(*b.belt_dir)
                                ));
                            } else {
                                line1.push_str(&format!(
                                    "POSER : {} ×{} — clic dans la scène",
                                    item.name(),
                                    stock
                                ));
                            }
                        }
                        if !line1.is_empty() {
                            ui.label(RichText::new(line1).size(13.0).strong());
                        }
                        ui.label(
                            RichText::new(
                                "1-9 : choisir · R : rotation · X : pelle · F : colis (dock sud) · E : ordinateur · Échap : souris libre · Espace : pause",
                            )
                            .size(12.0)
                            .weak(),
                        );
                    });
                });
        });
}

/// Prompt "[F] ..." au centre : le colis le plus proche est actionnable.
fn draw_floor_prompt(ctx: &egui::Context, b: &UiBorrow) {
    if b.computer.open {
        return;
    }
    if let Some(txt) = &b.floor_prompt {
        egui::Area::new(egui::Id::new("floor_prompt"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 120.0])
            .show(ctx, |ui| {
                egui::Frame::group(ui.style())
                    .fill(Color32::from_black_alpha(200))
                    .show(ui, |ui| {
                        ui.label(RichText::new(txt).size(18.0).color(terminal_green()).strong());
                    });
            });
    }
}

fn arrow(dir: h33_core::grid::Dir) -> &'static str {
    match dir {
        h33_core::grid::Dir::North => "^",
        h33_core::grid::Dir::East => ">",
        h33_core::grid::Dir::South => "v",
        h33_core::grid::Dir::West => "<",
    }
}

// ======================================================================
//  Prompt d'interaction + ordinateur HANGAR-OS
// ======================================================================

fn draw_computer_prompt(ctx: &egui::Context, b: &UiBorrow) {
    if !b.near_computer || b.computer.open {
        return;
    }
    egui::Area::new(egui::Id::new("computer_prompt"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 60.0])
        .show(ctx, |ui| {
            egui::Frame::group(ui.style())
                .fill(Color32::from_black_alpha(200))
                .show(ui, |ui| {
                    ui.label(RichText::new("[E] Utiliser l'ordinateur").size(20.0).color(terminal_green()).strong());
                });
        });
}

// ======================================================================
//  Bureau « HANGAR-OS 11 » : un faux Windows 11 dans le hangar.
//  Fond d'écran "bloom", icônes, barre des tâches centrée, menu Démarrer,
//  fenêtres d'apps (Boutique / Colis / Terminal) + Corbeille et Ce PC.
// ======================================================================

fn win_accent() -> Color32 {
    Color32::from_rgb(0, 120, 212) // bleu Windows
}

fn taskbar_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(22, 24, 30, 236)
}

fn mica_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(32, 34, 42, 248)
}

fn win_text() -> Color32 {
    Color32::from_rgb(235, 240, 250)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

fn draw_computer(ctx: &egui::Context, b: &mut UiBorrow) {
    // 1) Fond d'écran plein écran (sous la barre HUD du jeu).
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(5, 10, 30)))
        .show(ctx, |ui| {
            paint_wallpaper(ui);
            paint_desktop_icons(ui, b);
        });

    // 2) Fenêtres d'apps — plusieurs peuvent être ouvertes à la fois,
    //    comme sur un vrai bureau. Le X de la fenêtre ferme l'app.
    let mut shop = b.computer.apps[0];
    egui::Window::new("Boutique — HANGAR-OS Store")
        .open(&mut shop)
        .resizable(true)
        .default_width(640.0)
        .default_height(440.0)
        .frame(egui::Frame::default().fill(mica_fill()).inner_margin(egui::Margin::same(10)))
        .show(ctx, |ui| draw_shop_tab(ui, b));
    b.computer.apps[0] = shop;

    let mut packages = b.computer.apps[1];
    egui::Window::new("Colis — Livraisons")
        .open(&mut packages)
        .resizable(true)
        .default_width(520.0)
        .default_height(400.0)
        .frame(egui::Frame::default().fill(mica_fill()).inner_margin(egui::Margin::same(10)))
        .show(ctx, |ui| draw_packages_tab(ui, b));
    b.computer.apps[1] = packages;

    let mut terminal = b.computer.apps[2];
    egui::Window::new("Terminal — operateur@hangar33")
        .open(&mut terminal)
        .resizable(true)
        .default_size([680.0, 430.0])
        .frame(
            egui::Frame::default()
                .fill(Color32::from_rgb(12, 13, 16))
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ctx, |ui| draw_terminal_tab(ui, b));
    b.computer.apps[2] = terminal;

    // 3) Easter eggs : Corbeille + Ce PC (icônes du bureau).
    let mut trash = b.computer.trash_open;
    egui::Window::new("Corbeille")
        .open(&mut trash)
        .default_width(360.0)
        .frame(egui::Frame::default().fill(mica_fill()).inner_margin(egui::Margin::same(10)))
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("La corbeille est vide.").strong());
            ui.label("(on a vendu le contenu — c'est littéralement le jeu.)");
        });
    b.computer.trash_open = trash;

    let mut pc = b.computer.pc_open;
    egui::Window::new("Ce PC")
        .open(&mut pc)
        .default_width(430.0)
        .frame(egui::Frame::default().fill(mica_fill()).inner_margin(egui::Margin::same(10)))
        .show(ctx, |ui| {
            ui.label(RichText::new("HANGAR-PC").strong().size(18.0));
            ui.separator();
            ui.label("Processeur  : Bras robot x9 (0,25 s/cycle)");
            ui.label("Mémoire     : 500 000 000 colis adressables");
            ui.label("Disque C:   : 3,7 Po de cartons non ouverts");
            ui.label("Carte son   : vrombissement de générateur continu");
            ui.label("Système     : HANGAR-OS 11 (build 33.33)");
            ui.separator();
            ui.label(
                RichText::new("Besoin d'une VRAIE machine virtuelle ? Le vieux code admin du Terminal traîne toujours quelque part… (nom du jeu + numéro)")
                    .weak()
                    .small(),
            );
        });
    b.computer.pc_open = pc;

    // 4) Menu Démarrer (au-dessus des fenêtres).
    if b.computer.start_open {
        draw_start_menu(ctx, b);
    }

    // 5) Barre des tâches (toujours au-dessus).
    draw_taskbar(ctx, b);
}

/// Fond d'écran : dégradé bleu nuit + auréoles translucides (façon "bloom").
fn paint_wallpaper(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    let steps = 40;
    let strip_h = rect.height() / steps as f32;
    let top = Color32::from_rgb(5, 10, 30);
    let mid = Color32::from_rgb(0, 70, 150);
    let bottom = Color32::from_rgb(0, 120, 212);
    for i in 0..steps {
        let f = i as f32 / (steps - 1).max(1) as f32;
        let c = if f < 0.55 {
            lerp_color(top, mid, f / 0.55)
        } else {
            lerp_color(mid, bottom, (f - 0.55) / 0.45)
        };
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + i as f32 * strip_h),
                egui::vec2(rect.width(), strip_h + 1.0),
            ),
            egui::CornerRadius::ZERO,
            c,
        );
    }
    // « Bloom » : auréoles concentriques translucides au centre.
    let (cx, cy) = (rect.center().x, rect.center().y);
    let petals = [
        (0.0, 0.0, 210.0),
        (150.0, 30.0, 150.0),
        (-140.0, -70.0, 170.0),
        (60.0, -160.0, 130.0),
        (-80.0, 130.0, 140.0),
        (190.0, -60.0, 90.0),
    ];
    for (i, (dx, dy, r)) in petals.into_iter().enumerate() {
        let a = 30u8.saturating_sub((i as u8) * 4);
        painter.circle_filled(egui::pos2(cx + dx, cy + dy), r, Color32::from_rgba_unmultiplied(70, 150, 255, a));
        painter.circle_filled(
            egui::pos2(cx + dx, cy + dy),
            r * 0.6,
            Color32::from_rgba_unmultiplied(110, 185, 255, a.saturating_add(12)),
        );
    }
    painter.text(
        egui::pos2(rect.right() - 18.0, rect.bottom() - 72.0),
        egui::Align2::RIGHT_BOTTOM,
        "HANGAR-OS 11 — édition colis",
        egui::FontId::proportional(16.0),
        Color32::from_rgba_unmultiplied(255, 255, 255, 55),
    );
}

/// Icône vectorielle générique (pas d'emoji : les fontes egui n'en ont pas).
fn paint_icon(painter: &egui::Painter, kind: &str, c: egui::Pos2) {
    let s = egui::Stroke::new(2.2_f32, win_text());
    match kind {
        "start" => {
            // Logo : 4 carrés bleus.
            let q = 7.5;
            let g = 2.0;
            let off = |dx: f32, dy: f32| egui::pos2(c.x + dx - q - g / 2.0, c.y + dy - q - g / 2.0);
            for (dx, dy) in [(-(q + g), -(q + g)), (0.0, -(q + g)), (-(q + g), 0.0), (0.0, 0.0)] {
                painter.rect_filled(
                    egui::Rect::from_min_size(off(dx, dy), egui::vec2(q, q)),
                    egui::CornerRadius::same(1),
                    win_accent(),
                );
            }
        }
        "shop" => {
            // Storefront : auvent + corps.
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 4.0), egui::vec2(26.0, 16.0)),
                egui::CornerRadius::same(2),
                Color32::from_rgb(255, 200, 80),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y - 8.0), egui::vec2(30.0, 6.0)),
                egui::CornerRadius::same(2),
                win_accent(),
            );
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 4.0), egui::vec2(8.0, 8.0)),
                egui::CornerRadius::same(1),
                mica_fill(),
            );
        }
        "colis" => {
            // Carton : corps + ruban.
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(26.0, 22.0)),
                egui::CornerRadius::same(2),
                s,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([egui::pos2(c.x, c.y - 11.0), egui::pos2(c.x, c.y + 11.0)], s);
            painter.line_segment([egui::pos2(c.x - 13.0, c.y - 4.0), egui::pos2(c.x + 13.0, c.y - 4.0)], s);
        }
        "terminal" => {
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(28.0, 22.0)),
                egui::CornerRadius::same(2),
                Color32::from_rgb(14, 15, 18),
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(28.0, 22.0)),
                egui::CornerRadius::same(2),
                s,
                egui::StrokeKind::Middle,
            );
            painter.text(
                egui::pos2(c.x - 8.0, c.y),
                egui::Align2::LEFT_CENTER,
                ">_",
                egui::FontId::monospace(11.0),
                terminal_green(),
            );
        }
        "trash" => {
            painter.rect_stroke(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 5.0), egui::vec2(26.0, 26.0)),
                egui::CornerRadius::same(3),
                s,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([egui::pos2(c.x - 15.0, c.y - 8.0), egui::pos2(c.x + 15.0, c.y - 8.0)], s);
            painter.line_segment([egui::pos2(c.x - 7.0, c.y - 8.0), egui::pos2(c.x - 7.0, c.y - 14.0)], s);
            painter.line_segment([egui::pos2(c.x + 7.0, c.y - 8.0), egui::pos2(c.x + 7.0, c.y - 14.0)], s);
            for dx in [-8.0, 0.0, 8.0] {
                painter.line_segment([egui::pos2(c.x + dx, c.y + 0.0), egui::pos2(c.x + dx, c.y + 13.0)], s);
            }
        }
        "pc" => {
            painter.rect_stroke(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y - 2.0), egui::vec2(40.0, 26.0)),
                egui::CornerRadius::same(3),
                s,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([egui::pos2(c.x - 8.0, c.y + 13.0), egui::pos2(c.x + 8.0, c.y + 13.0)], s);
            painter.line_segment([egui::pos2(c.x, c.y + 13.0), egui::pos2(c.x, c.y + 19.0)], s);
            painter.line_segment([egui::pos2(c.x - 14.0, c.y + 19.0), egui::pos2(c.x + 14.0, c.y + 19.0)], s);
        }
        _ => {}
    }
}

/// Une icône de bureau cliquable (étiquette blanche sous l'icône).
fn desktop_icon(ui: &mut egui::Ui, top_left: egui::Pos2, kind: &str, label: &str) -> egui::Response {
    let rect = egui::Rect::from_min_size(top_left, egui::vec2(92.0, 98.0));
    let resp = ui.allocate_rect(rect, egui::Sense::click());
    let painter = ui.painter();
    if resp.hovered() || resp.clicked() {
        painter.rect_filled(rect, egui::CornerRadius::same(6), Color32::from_rgba_unmultiplied(255, 255, 255, 28));
    }
    paint_icon(painter, kind, egui::pos2(rect.center().x, rect.top() + 36.0));
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 12.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        win_text(),
    );
    resp
}

fn paint_desktop_icons(ui: &mut egui::Ui, b: &mut UiBorrow) {
    let base = ui.max_rect().left_top() + egui::vec2(18.0, 18.0);
    if desktop_icon(ui, base, "pc", "Ce PC").clicked() {
        b.computer.pc_open = true;
        b.computer.start_open = false;
    }
    if desktop_icon(ui, base + egui::vec2(0.0, 106.0), "trash", "Corbeille").clicked() {
        b.computer.trash_open = true;
        b.computer.start_open = false;
    }
}

/// Barre des tâches : bouton Démarrer + apps centrées, horloge à droite.
fn draw_taskbar(ctx: &egui::Context, b: &mut UiBorrow) {
    const BAR_H: f32 = 56.0;
    const BTN: f32 = 42.0;
    egui::Area::new(egui::Id::new("h33_taskbar"))
        .anchor(egui::Align2::LEFT_BOTTOM, [0.0, 0.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let screen = ctx.screen_rect();
            let rect = egui::Rect::from_min_size(
                egui::pos2(screen.left(), screen.bottom() - BAR_H),
                egui::vec2(screen.width(), BAR_H),
            );
            ui.allocate_rect(rect, egui::Sense::hover());
            let painter = ui.painter().clone();
            painter.rect_filled(
                rect,
                egui::CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 },
                taskbar_fill(),
            );

            // Groupe centré : Démarrer + 3 apps.
            let apps: [(&str, &str, usize); 3] = [
                ("shop", "Boutique", 0),
                ("colis", "Colis", 1),
                ("terminal", "Terminal", 2),
            ];
            let group_w = BTN + 6.0 + apps.len() as f32 * (BTN + 6.0);
            let mut x = rect.center().x - group_w / 2.0;

            // Démarrer.
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 7.0), egui::vec2(BTN, BTN));
            let resp = ui.allocate_rect(r, egui::Sense::click());
            let hl = resp.hovered() || b.computer.start_open;
            if hl {
                painter.rect_filled(r, egui::CornerRadius::same(6), Color32::from_rgba_unmultiplied(255, 255, 255, 26));
            }
            paint_icon(&painter, "start", r.center());
            if resp.clicked() {
                b.computer.start_open = !b.computer.start_open;
            }
            resp.on_hover_text("Démarrer");
            x += BTN + 6.0;

            for (kind, label, idx) in apps {
                let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 7.0), egui::vec2(BTN, BTN));
                let resp = ui.allocate_rect(r, egui::Sense::click());
                let active = b.computer.apps[idx];
                if resp.hovered() || active {
                    painter.rect_filled(r, egui::CornerRadius::same(6), Color32::from_rgba_unmultiplied(255, 255, 255, 26));
                }
                paint_icon(&painter, kind, r.center());
                if active {
                    // Indicateur d'app ouverte : petit trait accent sous le bouton.
                    painter.line_segment(
                        [egui::pos2(r.center().x - 7.0, r.bottom() + 3.0), egui::pos2(r.center().x + 7.0, r.bottom() + 3.0)],
                        egui::Stroke::new(3.0_f32, win_accent()),
                    );
                }
                if resp.clicked() {
                    b.computer.apps[idx] = !b.computer.apps[idx];
                    b.computer.start_open = false;
                }
                resp.on_hover_text(label);
                x += BTN + 6.0;
            }

            // Horloge (heure de hangar = 09:00 + temps de sim).
            let elapsed = b.sim.as_ref().map_or(0.0, |s| s.elapsed_s);
            let total_s = 9 * 3600 + elapsed as u64;
            let (h, m, s) = ((total_s / 3600) % 24, (total_s / 60) % 60, total_s % 60);
            painter.text(
                egui::pos2(rect.right() - 16.0, rect.center().y - 8.0),
                egui::Align2::RIGHT_CENTER,
                format!("{h:02}:{m:02}:{s:02}"),
                egui::FontId::monospace(15.0),
                win_text(),
            );
            painter.text(
                egui::pos2(rect.right() - 16.0, rect.center().y + 9.0),
                egui::Align2::RIGHT_CENTER,
                "HANGAR-OS 11 · Échap : hangar",
                egui::FontId::proportional(10.5),
                Color32::from_rgba_unmultiplied(255, 255, 255, 120),
            );
        });
}

/// Menu Démarrer : recherche, apps épinglées, utilisateur, éteindre.
fn draw_start_menu(ctx: &egui::Context, b: &mut UiBorrow) {
    egui::Area::new(egui::Id::new("h33_start"))
        .anchor(egui::Align2::LEFT_BOTTOM, [10.0, -64.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(mica_fill())
                .corner_radius(egui::CornerRadius::same(10))
                .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22)))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(520.0, 380.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut b.computer.start_search)
                            .hint_text("Rechercher une app…")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(12.0);
                    ui.label(RichText::new("Épinglé").strong().size(14.0));
                    ui.add_space(6.0);

                    // Grille 4 tuiles : 3 apps + Corbeille.
                    let tiles: [(&str, &str, &str); 4] = [
                        ("shop", "Boutique", "Store du hangar"),
                        ("colis", "Colis", "Livraisons"),
                        ("terminal", "Terminal", "Ligne de commande"),
                        ("trash", "Corbeille", "Vide, promis"),
                    ];
                    let tile = egui::vec2(116.0, 96.0);
                    for row in 0..2 {
                        ui.horizontal(|ui| {
                            for col in 0..2 {
                                let (kind, label, sub) = tiles[row * 2 + col];
                                let (r_rect, r) = ui.allocate_exact_size(tile, egui::Sense::click());
                                let painter = ui.painter().clone();
                                if r.hovered() {
                                    painter.rect_filled(r_rect, egui::CornerRadius::same(6), Color32::from_rgba_unmultiplied(255, 255, 255, 18));
                                }
                                paint_icon(&painter, kind, egui::pos2(r_rect.center().x, r_rect.top() + 30.0));
                                painter.text(
                                    egui::pos2(r_rect.center().x, r_rect.bottom() - 30.0),
                                    egui::Align2::CENTER_CENTER,
                                    label,
                                    egui::FontId::proportional(13.0),
                                    win_text(),
                                );
                                painter.text(
                                    egui::pos2(r_rect.center().x, r_rect.bottom() - 14.0),
                                    egui::Align2::CENTER_CENTER,
                                    sub,
                                    egui::FontId::proportional(10.0),
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 130),
                                );
                                if r.clicked() {
                                    match kind {
                                        "shop" => b.computer.apps[0] = true,
                                        "colis" => b.computer.apps[1] = true,
                                        "terminal" => b.computer.apps[2] = true,
                                        _ => b.computer.trash_open = true,
                                    }
                                    b.computer.start_open = false;
                                }
                            }
                        });
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Operateur 33").strong());
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Éteindre").clicked() {
                                b.computer.start_open = false;
                                b.computer.close_request = true;
                            }
                        });
                    });
                });
        });
}

/// Onglet Boutique : commander les machines/tapis (payé maintenant, livré).
fn draw_shop_tab(ui: &mut egui::Ui, b: &mut UiBorrow) {
    ui.horizontal(|ui| {
        ui.label("Quantité :");
        ui.add(egui::DragValue::new(&mut b.computer.order_qty).range(1..=100).suffix(" ×"));
        ui.label(RichText::new("Payé à la commande, livraison ~8 s sur le quai.").weak().small());
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let Some(sim) = b.sim.as_mut() else { return };
        for item in ALL_SHOP_ITEMS {
            let si = item.stock_index();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_width(ui.available_width() - 8.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(item.name()).strong());
                            ui.label(RichText::new(fmt_eur(item.cost())).color(money_color()));
                            if sim.stock[si] > 0 {
                                ui.label(RichText::new(format!("stock : {}", sim.stock[si])).weak().small());
                            }
                        });
                        ui.label(RichText::new(shop_desc(item)).weak().small());
                    });
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let total = item.cost() * b.computer.order_qty as f64;
                        let afford = sim.eco.money >= total;
                        let btn = egui::Button::new(format!("Commander ({} €)", fmt_eur(total)));
                        if ui.add_enabled(afford, btn).clicked() {
                            let qty = b.computer.order_qty;
                            match sim.order(item, qty) {
                                Ok(()) => b.computer.log.push(format!(
                                    "$ order {} x{} -- ok, livraison dans ~8 s",
                                    cmd_name(item), qty
                                )),
                                Err(e) => b.computer.log.push(format!("$ order {} -- ERREUR: {e}", cmd_name(item))),
                            }
                        }
                    });
                });
            });
            ui.add_space(2.0);
        }
    });
}

fn shop_desc(item: ShopItem) -> &'static str {
    match item {
        ShopItem::Belt => "Transporte les objets sur une case. La base de toute ligne.",
        ShopItem::Machine(MachineKind::AutoBuyer) => "Achète des colis tout seul et les pose sur la ligne.",
        ShopItem::Machine(MachineKind::Unpacker) => "Ouvre les colis : contenu + carton. 2,5 s par colis.",
        ShopItem::Machine(MachineKind::Seller) => "VEND TOUT ce qui arrive. Y compris la bague. Prudence.",
        ShopItem::Machine(MachineKind::Splitter) => "Répartit alternativement vers ses deux sorties latérales.",
        ShopItem::Machine(MachineKind::Incinerator) => "Brûle tout. Carton = kW gratuits. Bague = catastrophe.",
        ShopItem::Machine(MachineKind::RingScanner) => "Détecte LA BAGUE et la sécurise au coffre (victoire).",
        ShopItem::Machine(MachineKind::RobotArm) => "Manipulateur universel : accepte tout, tourne en coin, insère direct dans les machines.",
        ShopItem::Machine(MachineKind::Generator) => "Générateur diesel : 6 kW gratuits en continu. Adieu la facture réseau.",
        #[allow(unreachable_patterns)]
        _ => "",
    }
}

fn cmd_name(item: ShopItem) -> &'static str {
    match item {
        ShopItem::Belt => "belt",
        ShopItem::Machine(k) => match k {
            MachineKind::AutoBuyer => "autobuyer",
            MachineKind::Unpacker => "unpacker",
            MachineKind::Seller => "seller",
            MachineKind::Splitter => "splitter",
            MachineKind::Incinerator => "incinerator",
            MachineKind::RingScanner => "scanner",
            MachineKind::RobotArm => "robotarm",
            MachineKind::Generator => "generator",
        },
    }
}

/// Onglet Colis : commande groupée livrée sur la zone de palettes.
fn draw_packages_tab(ui: &mut egui::Ui, b: &mut UiBorrow) {
    ui.label("Commande de colis mystères — livraison sur la zone de palettes.");
    ui.add_space(4.0);
    let Some(sim) = b.sim.as_mut() else { return };
    let mut qty = b.computer.package_qty as f64;
    ui.add(egui::Slider::new(&mut qty, 1.0..=200.0).logarithmic(true).text("quantité"));
    b.computer.package_qty = qty.round() as u64;
    let n = b.computer.package_qty;
    let total = balance::PACKAGE_PRICE_EUR * n as f64;
    let eta_txt = format!(
        "~{:.0} s",
        h33_core::shop::DELIVERY_PACKAGE_BASE_S + h33_core::shop::DELIVERY_PACKAGE_PER_UNIT_S * n as f64
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let afford = sim.eco.money >= total;
        if ui.add_enabled(afford, egui::Button::new(format!("Commander {n} colis ({})", fmt_eur(total)))).clicked() {
            match sim.order_packages(n) {
                Ok(()) => b.computer.log.push(format!("$ order colis x{n} -- ok, livraison {eta_txt}")),
                Err(e) => b.computer.log.push(format!("$ order colis -- ERREUR: {e}")),
            }
        }
        ui.label(RichText::new(format!("12 €/colis · livraison {eta_txt} · marge EV ~+50 %")).weak().small());
    });
    ui.separator();
    // Livraisons en cours.
    if sim.deliveries.is_empty() {
        ui.weak("Aucune livraison en cours.");
    } else {
        ui.label(RichText::new("Livraisons en cours :").strong());
        for d in &sim.deliveries {
            let remaining = (d.eta_s - sim.elapsed_s).max(0.0);
            let what = match d.item {
                DeliveryKind::Stock(item, q) => format!("{}× {}", q, item.name()),
                DeliveryKind::Packages(n) => format!("{} colis", n),
            };
            ui.label(format!("  · {what} — dans {remaining:.0} s"));
        }
    }
    if sim.pending_packages > 0 {
        ui.label(
            RichText::new(format!("{} colis attendent un slot de palette libre.", sim.pending_packages))
                .color(sev_color(Severity::Warn))
                .small(),
        );
    }
}

/// Onglet Terminal : ligne de commande + CODE SECRET (vraie VM !).
fn draw_terminal_tab(ui: &mut egui::Ui, b: &mut UiBorrow) {
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in &b.computer.log {
                ui.label(RichText::new(line).color(terminal_green()).size(13.0).monospace());
            }
        });
    ui.separator();
    let response = ui.add(
        egui::TextEdit::singleline(&mut b.computer.input)
            .hint_text("tape une commande… (help)")
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Monospace),
    );
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let clicked = ui.button("Envoyer").clicked();
    if enter && response.has_focus() || clicked {
        let cmd = b.computer.input.trim().to_string();
        b.computer.input.clear();
        if !cmd.is_empty() {
            let qty = b.computer.package_qty;
            let mut io = TermIo {
                sim: b.sim,
                log: &mut b.computer.log,
                toasts: b.toasts,
                package_qty: qty,
            };
            terminal_exec(cmd, &mut io);
        }
        ui.ctx().request_repaint();
    }
}

// ======================================================================
//  Terminal HANGAR-OS : exécution réutilisable (UI + smoke-test)
// ======================================================================

/// Entrées/sorties du terminal, détachées de l'UI egui : le smoke-test
/// peut taper de VRAIES commandes sans passer par le rendu.
pub(crate) struct TermIo<'a> {
    pub sim: &'a mut Option<Sim>,
    pub log: &'a mut Vec<String>,
    pub toasts: &'a mut Vec<(String, Severity, f64)>,
    pub package_qty: u64,
}

/// Exécution des commandes du terminal HANGAR-OS.
pub(crate) fn terminal_exec(cmd: String, io: &mut TermIo) {
    io.log.push(format!("$ {cmd}"));
    let lower = cmd.to_ascii_lowercase();

    // --- CODE SECRET : lance une VRAIE VM sur la machine hôte ---
    // "le nom du jeu + le numéro du jeu" -> HANGAR33 (ou HANGAR-33)
    if lower.replace('-', "") == "hangar33" || lower == "hangar 33" {
        io.log.push("[sudo] mot de passe d'opérateur : ********".into());
        io.log.push("[sudo] autorisation spéciale HANGAR-OS : hyperviseur déverrouillé…".into());
        match crate::vm::launch_real_vm() {
            Ok(msg) => {
                io.log.push(msg);
                let now = io.sim.as_ref().map_or(0.0, |s| s.elapsed_s);
                io.toasts.push((
                    "VM lancée depuis HANGAR-OS — c'est une VRAIE machine virtuelle.".into(),
                    Severity::Info,
                    now + 6.0,
                ));
            }
            Err(e) => {
                io.log.push(format!("erreur : {e}"));
                let now = io.sim.as_ref().map_or(0.0, |s| s.elapsed_s);
                io.toasts.push((e, Severity::Warn, now + 6.0));
            }
        }
        return;
    }

    let Some(sim) = io.sim.as_mut() else { return };
    let mut parts = lower.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("help"), _, _) => {
            io.log.push("commandes :".into());
            io.log.push("  status              — argent, pool, électricité".into());
            io.log.push("  stock               — stock livré par article".into());
            io.log.push("  order <item> [qty]  — commander (belt, autobuyer, unpacker, seller,".into());
            io.log.push("                        splitter, incinerator, scanner, robotarm, generator)".into());
            io.log.push("  order colis <n>     — commander n colis sur les palettes".into());
            io.log.push("  clear               — vide l'écran".into());
            io.log.push("  … et il paraît qu'un vieux code admin traîne encore. (nom du jeu + numéro)".into());
        }
        (Some("status"), _, _) => {
            io.log.push(format!(
                "argent {:.2} € · pool {} / {} colis · black-out {}",
                sim.eco.money,
                h33_core::sim::format_count(sim.eco.packages_left),
                h33_core::sim::format_count(sim.eco.total_packages),
                if sim.blackout { "OUI" } else { "non" }
            ));
            let (demand, free) = sim.power_summary();
            io.log.push(format!("élec : {demand:.1} kW demandés, {free:.1} kW gratuits"));
        }
        (Some("stock"), _, _) => {
            for item in ALL_SHOP_ITEMS {
                io.log.push(format!("  {:<14} ×{}", cmd_name(item), sim.stock[item.stock_index()]));
            }
        }
        (Some("order"), Some(what), rest) => {
            if what == "colis" || what == "packages" {
                let n: u64 = rest.and_then(|r| r.parse().ok()).unwrap_or(io.package_qty);
                match sim.order_packages(n) {
                    Ok(()) => io.log.push(format!(
                        "commande : {n} colis payés, livraison en route vers la zone de palettes (mur SUD)."
                    )),
                    Err(e) => io.log.push(format!("erreur : {e}")),
                }
                return;
            }
            let item = ALL_SHOP_ITEMS.iter().copied().find(|i| cmd_name(*i) == what);
            let qty: u32 = rest.and_then(|r| r.parse().ok()).unwrap_or(1);
            match item {
                Some(item) => match sim.order(item, qty) {
                    Ok(()) => io.log.push(format!(
                        "commande : {}× {} — payé, livraison ~8 s",
                        qty, item.name()
                    )),
                    Err(e) => io.log.push(format!("erreur : {e}")),
                },
                None => io.log.push(format!(
                    "article inconnu : {what} — voir 'order <item>' (help)"
                )),
            }
        }
        (Some("clear"), _, _) => {
            io.log.clear();
        }
        _ => {
            io.log.push("commande inconnue — 'help' liste ce que HANGAR-OS avoue.".into());
        }
    }
}

// ======================================================================
//  Panneau machine sélectionnée
// ======================================================================

fn draw_selected_panel(ctx: &egui::Context, b: &mut UiBorrow) {
    let Some(mi) = *b.selected else { return };
    let Some(sim) = b.sim.as_mut() else { return };
    let Some(m) = sim.machines.get(mi) else {
        *b.selected = None;
        return;
    };
    egui::Window::new(format!("Machine : {}", m.kind.spec().name))
        .default_width(280.0)
        .show(ctx, |ui| {
            let m = sim.machines.get_mut(mi).unwrap();
            ui.label(format!("Traités depuis l'installation : {}", m.processed));
            if let Some(item) = &m.busy_with {
                ui.label(format!("En cours : {}", item.kind.def().name_fr));
            }
            match m.kind {
                MachineKind::AutoBuyer => {
                    ui.add_space(6.0);
                    ui.label("Période d'achat :");
                    ui.add(
                        egui::Slider::new(
                            &mut m.auto_buy_period_s,
                            balance::AUTO_BUY_PERIOD_MIN_S..=balance::AUTO_BUY_PERIOD_MAX_S,
                        )
                        .suffix(" s")
                        .logarithmic(true),
                    );
                    ui.label(
                        RichText::new(format!(
                            "≈ {:.0} colis/min → {:.0} €/min d'achat",
                            60.0 / m.auto_buy_period_s,
                            60.0f64 / m.auto_buy_period_s as f64 * balance::PACKAGE_PRICE_EUR
                        ))
                        .weak()
                        .small(),
                    );
                }
                MachineKind::Incinerator => {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "ATTENTION : brûle TOUT ce qui arrive. Carton -> électricité gratuite. Autre -> perdu. LA BAGUE -> catastrophe.",
                        )
                        .color(sev_color(Severity::Danger))
                        .small(),
                    );
                }
                MachineKind::Seller => {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "ATTENTION : vend TOUT ce qui arrive, y compris la bague si elle n'a pas été interceptée par le scanner en amont !",
                        )
                        .color(sev_color(Severity::Warn))
                        .small(),
                    );
                }
                MachineKind::RobotArm => {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Bras universel : accepte tout, pousse devant lui (tapis OU machine). 0,25 s par manipulation.")
                            .weak()
                            .small(),
                    );
                }
                MachineKind::Generator => {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "Produit {:.1} kW gratuits en continu (vrombissement garanti).",
                            balance::GENERATOR_FREE_KW
                        ))
                        .color(Color32::from_rgb(255, 160, 90))
                        .small(),
                    );
                }
                _ => {}
            }
            ui.add_space(6.0);
            if ui.button(RichText::new("Demolir (50 % remboursé)")).clicked() {
                let key = sim.machine_cell[mi];
                let (cx, cy) = h33_core::sim::cell_xy(key);
                let _ = sim.demolish_cell(cx, cy);
                *b.selected = None;
            }
        });
}

// ======================================================================
//  Toasts + aide + victoire
// ======================================================================

fn draw_toasts(ctx: &egui::Context, b: &mut UiBorrow) {
    let toasts: Vec<(String, Severity)> = b.toasts.iter().map(|(t, s, _)| (t.clone(), *s)).collect();
    if toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -110.0])
        .show(ctx, |ui| {
            for (text, sev) in toasts.iter().rev() {
                egui::Frame::group(ui.style())
                    .fill(Color32::from_black_alpha(220))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).color(sev_color(*sev)));
                    });
            }
        });
}

fn draw_help(ctx: &egui::Context, b: &mut UiBorrow) {
    if !*b.show_help {
        return;
    }
    egui::Window::new("Aide — HANGAR 33 v0.3").show(ctx, |ui| {
        ui.label(RichText::new("Objectif").strong());
        ui.label("Une bague rarissime est cachée dans UN colis parmi des millions. Trouve-la et sécurise-la au coffre (scanner à bague, ou touche F devant le colis ouvert — dock sud).");
        ui.add_space(6.0);
        ui.label(RichText::new("L'ordinateur (E)").strong());
        ui.label("Va au bureau sud-ouest et appuie sur E : HANGAR-OS s'ouvre. Onglet Boutique : commander machines et tapis (payé à la commande, livraison ~8 s). Onglet Colis : commande groupée livrée sur les palettes. Onglet Terminal : lignes de commande… et il paraît qu'un vieux code admin y traîne.");
        ui.add_space(6.0);
        ui.label(RichText::new("Boucle").strong());
        ui.label("Commander des colis -> ouvrir -> vendre le contenu -> automatiser (Auto-Acheteur -> tapis/bras robots -> Dépaqueteur -> ... -> Guichet). Colis du dock : F pour ouvrir, F pour vendre (Shift+F : revente à l'aveugle). Poser le stock livré : touches 1-9 puis clic (R : rotation).");
        ui.add_space(6.0);
        ui.label(RichText::new("Électricité").strong());
        ui.label("Les machines consomment des kW, facturés 0,15 €/kWh sur le réseau. Le GÉNÉRATEUR fournit 6 kW gratuits. L'incinérateur produit des kW gratuits en brûlant les cartons... mais brûle TOUT ce qui arrive, y compris la bague. Construis un TRIEUR avant l'incinérateur.");
        ui.add_space(6.0);
        ui.label(RichText::new("La bague").strong());
        ui.label("- Vendue (manuellement ou par un guichet) : -25 % du capital, elle se recache dans les colis non achetés.");
        ui.label("- Incinérée : elle se recache aussi (le fournisseur la remplace).");
        ui.label("- Sécurisée (scanner à bague ou touche F devant le colis) : VICTOIRE.");
        ui.add_space(6.0);
        ui.label(RichText::new("Contrôles").strong());
        ui.label("ZQSD/WASD : marcher · Souris : regarder · Shift : courir · E : ordinateur (près du bureau) · 1-9 : choisir à poser · Clic : poser / sélectionner · F : colis du dock (Shift+F : revente à l'aveugle) · R : rotation · X : pelle · Échap : souris libre · Espace : pause");
        ui.add_space(6.0);
        ui.label(RichText::new("Idle : 0 gain hors-ligne — le hangar ne travaille que fenêtre ouverte.").weak().small());
    });
}

/// Overlay de victoire (bague sécurisée).
fn draw_victory(ctx: &egui::Context, sim: &Sim, back_to_menu: &mut bool) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_black_alpha(190)))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.2);
                ui.heading(RichText::new("VICTOIRE").size(64.0).color(gold()));
                ui.label(RichText::new("La bague est au coffre.").size(20.0));
                ui.add_space(16.0);
                ui.label(format!(
                    "Colis achetés : {} / {} ({:.2} %) · Ventes totales : {} · Réseau : {:.2} kWh",
                    h33_core::sim::format_count(sim.eco.packages_bought),
                    h33_core::sim::format_count(sim.eco.total_packages),
                    sim.eco.pool_progress() * 100.0,
                    fmt_eur(sim.eco.total_earned_eur),
                    sim.eco.grid_kwh_bought,
                ));
                ui.add_space(20.0);
                if ui.button(RichText::new("Nouvelle partie").size(20.0)).clicked() {
                    *back_to_menu = true;
                }
            });
        });
}
