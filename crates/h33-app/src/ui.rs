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
    draw_hint(ctx, b);
    draw_selected_panel(ctx, b);
    draw_toasts(ctx, b);
    draw_help(ctx, b);
    draw_computer_prompt(ctx, b);
    draw_floor_prompt(ctx, b);
    draw_computer(ctx, b);
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
                ui.label(
                    RichText::new(format!("Livraisons en cours : {}", sim.deliveries.len()))
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

fn draw_computer(ctx: &egui::Context, b: &mut UiBorrow) {
    if !b.computer.open {
        return;
    }
    let tabs = ["Boutique", "Colis", "Terminal"];
    egui::Window::new(RichText::new("HANGAR-OS 33 — terminal de gestion").strong().color(terminal_green()))
        .default_width(720.0)
        .default_height(480.0)
        .collapsible(false)
        .show(ctx, |ui| {
            let money = b.sim.as_ref().map_or(0.0, |s| s.eco.money);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Solde : {}", fmt_eur(money))).color(money_color()).strong());
                ui.separator();
                for (i, t) in tabs.iter().enumerate() {
                    if ui.selectable_label(b.computer.tab == i, RichText::new(*t).strong()).clicked() {
                        b.computer.tab = i;
                    }
                    ui.separator();
                }
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Échap : quitter").weak().small());
                });
            });
            ui.separator();
            match b.computer.tab {
                0 => draw_shop_tab(ui, b),
                1 => draw_packages_tab(ui, b),
                _ => draw_terminal_tab(ui, b),
            }
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
                    Ok(()) => io.log.push(format!("commande : {n} colis payés, livraison en route.")),
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
