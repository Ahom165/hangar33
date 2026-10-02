//! HANGAR 33 — binaire de jeu.
//!
//! Flags utiles (tests CI / lavapipe) :
//!   --smoke-test          lance la partie auto-scriptée et sort à 0
//!   --screenshot <path>   capture un PNG offscreen pendant le smoke-test
//!   --frames <n>          quitte après n frames
//!   --total <n>           nombre de colis (1 000 000 .. 500 000 000)
//!   --seed <hex64>        seed fixe (reproductibilité)

mod app;
mod ui;
mod vm;

use app::{App, AppOptions};
use std::path::PathBuf;

fn main() {
    let opts = AppOptions::parse(std::env::args().skip(1));
    let mut app = App::new(opts);
    let event_loop = winit::event_loop::EventLoop::builder()
        .build()
        .expect("initialisation de la boucle d'événements");
    event_loop.run_app(&mut app).expect("boucle d'événements interrompue");
}

impl AppOptions {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut opts = Self::default();
        let mut it = args;
        while let Some(a) = it.next() {
            match a.as_str() {
                "--smoke-test" => opts.smoke_test = true,
                "--screenshot" => {
                    if let Some(p) = it.next() {
                        opts.screenshot = Some(PathBuf::from(p));
                    }
                }
                "--frames" => {
                    if let Some(n) = it.next().and_then(|n| n.parse::<u64>().ok()) {
                        opts.frames = Some(n);
                    }
                }
                "--total" => {
                    if let Some(n) = it.next().and_then(|n| n.parse::<u64>().ok()) {
                        opts.total_packages = Some(n.clamp(
                            h33_core::balance::MIN_PACKAGES,
                            h33_core::balance::MAX_PACKAGES,
                        ));
                    }
                }
                "--seed" => {
                    if let Some(hex) = it.next() {
                        if let Some(seed) = h33_core::rng::parse_seed(&hex) {
                            opts.seed = Some(seed);
                        } else {
                            eprintln!("seed invalide (attendu : 64 chars hex)");
                        }
                    }
                }
                other => eprintln!("argument inconnu : {other}"),
            }
        }
        opts
    }
}
