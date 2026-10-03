//! Tests d'intégration de la simulation — la boucle de gameplay complète.
//!
//! Tous les tests utilisent des seeds FIXES : déterministes et reproductibles.
//! Pour "forcer" la bague tôt, on cherche une seed adéquate au runtime
//! (brute force triviale sur 256 seeds).

use h33_core::balance;
use h33_core::economy::RingState;
use h33_core::grid::Dir;
use h33_core::items::ItemKind;
use h33_core::machines::MachineKind;
use h33_core::rng::{GameRng, seed_fingerprint};
use h33_core::sim::{GameEvent, Sim};

/// Cherche une seed dont la bague est dans l'un des 64 premiers colis.
/// Renvoie (seed, position). La recherche est déterministe.
fn seed_avec_bague_tot() -> ([u8; 32], u64) {
    for k in 0u32..1_000_000 {
        let mut seed = [0u8; 32];
        seed[0..4].copy_from_slice(&k.to_le_bytes());
        let mut rng = GameRng::from_seed(seed);
        let slot = rng.range_u64(0..1_000_000);
        if slot < 64 {
            return (seed, slot);
        }
    }
    panic!("aucune seed avec bague parmi les 64 premiers colis");
}

/// Stock gratuit pour les tests de mécanique (depuis v0.3, le placement
/// consomme un stock LIVRÉ par l'ordinateur — ces tests testent autre chose).
fn remplir_stock(sim: &mut Sim) {
    for item in ALL_SHOP_ITEMS {
        sim.stock[item.stock_index()] += 50;
    }
}

#[test]
fn boucle_manuelle_achat_ouverture_vente() {
    let mut sim = Sim::new([7u8; 32], 1_000_000);
    let start = sim.eco.money;

    // Vague de déballage manuel : achète `n`, ouvre tout, vend tout,
    // brûle les cartons. Respecte FLOOR_PACKAGE_LIMIT et l'auto-nettoyage
    // des colis vides (les indices bougent -> on travaille en tête de zone).
    fn vague(sim: &mut Sim, n: usize) -> f64 {
        let mut sold = 0.0;
        for _ in 0..n {
            assert!(sim.manual_buy_package(), "achat manuel a échoué");
        }
        while !sim.floor.is_empty() {
            let pi = sim.floor.len() - 1;
            let items = sim.manual_open(pi).expect("ouverture");
            assert!(!items.is_empty(), "un colis vide ?!");
            while sim
                .floor
                .get(pi)
                .and_then(|p| p.items.as_ref())
                .is_some_and(|v| !v.is_empty())
            {
                let kind = sim.floor[pi].items.as_ref().unwrap()[0];
                if kind == ItemKind::CartonVide {
                    assert!(sim.manual_burn_item(pi, 0));
                } else {
                    sold += sim.manual_sell_item(pi, 0).expect("vente");
                }
            }
        }
        assert!(sim.floor.is_empty(), "zone de palettes mal nettoyée");
        sold
    }

    let total_purchases = 24 + 12;
    let mut sold_eur = vague(&mut sim, 24);
    sold_eur += vague(&mut sim, 12);

    assert_eq!(sim.eco.packages_bought, total_purchases);
    let spent = (start + 0.0) - sim.eco.money + sold_eur;
    assert!(spent > 0.0);
    // Équilibre économique : dépensé ~432 €, récupéré la revente.
    assert!(sold_eur > 0.0);
    assert!(sim.eco.money > 0.0, "blackout pendant le test ?!");
    let expected_kwh = total_purchases as f64 * balance::CARDBOARD_ENERGY_KWH;
    assert!(
        (sim.eco.energy_burned_kwh - expected_kwh).abs() < 1e-9,
        "énergie brûlée: {} != {}",
        sim.eco.energy_burned_kwh,
        expected_kwh
    );
}

#[test]
fn vente_de_la_bague_perd_un_quart_et_recache() {
    let (seed, slot) = seed_avec_bague_tot();
    let mut sim = Sim::new(seed, 1_000_000);

    // Achète les colis 0..=slot — le dernier contient la bague.
    for _ in 0..=slot {
        sim.manual_buy_package();
    }
    sim.manual_open(slot as usize).expect("ouverture du colis bague");
    let items = sim.floor[slot as usize].items.clone().unwrap();
    assert_eq!(items[0], ItemKind::Bague, "le colis {slot} doit contenir la bague");

    // La vend volontairement (le joueur cupide).
    let capital_avant = sim.eco.money;
    sim.manual_sell_item(slot as usize, 0).expect("vente bague");

    // 1) Pénalité : exactement 1/4 du capital.
    let capital_apres = capital_avant * 0.75;
    assert!((sim.eco.money - capital_apres).abs() < 1e-6);
    assert_eq!(sim.eco.rings_sold, 1);

    // 2) Elle s'est recachée STRICTEMENT après le dernier colis acheté.
    assert!(sim.eco.ring_slot >= sim.eco.next_package_id);
    assert_eq!(sim.eco.ring_state, RingState::Hidden);

    // 3) Un event RingSold a été émis.
    assert!(sim.events.iter().any(|e| matches!(e, GameEvent::RingSold { .. })));
}

#[test]
fn bague_brulee_recache_et_victoire_si_secourisee() {
    let (seed, slot) = seed_avec_bague_tot();
    let mut sim = Sim::new(seed, 1_000_000);
    for _ in 0..=slot {
        sim.manual_buy_package();
    }
    sim.manual_open(slot as usize).unwrap();
    // Brûle la bague (erreur de tri manuelle).
    assert!(sim.manual_burn_item(slot as usize, 0));
    assert_eq!(sim.eco.rings_burned, 1);
    assert!(sim.eco.ring_slot >= sim.eco.next_package_id);

    // ...le fournisseur la remplace : on triche en la rapprochant pour
    // vérifier le chemin "re-trouvée -> sécurisée -> victoire".
    let mut safety = 0;
    loop {
        sim.eco.ring_slot = sim.eco.next_package_id;
        assert!(sim.manual_buy_package());
        let last = sim.floor.len() - 1;
        sim.manual_open(last).unwrap();
        let items = sim.floor[last].items.clone().unwrap();
        if items.contains(&ItemKind::Bague) {
            let idx = items.iter().position(|k| *k == ItemKind::Bague).unwrap();
            assert!(sim.manual_secure_ring(last, idx));
            assert_eq!(sim.eco.ring_state, RingState::Secured);
            assert!(sim.events.iter().any(|e| matches!(e, GameEvent::RingSecured)));
            break;
        }
        safety += 1;
        assert!(safety < 100, "boucle infinie sur la recherche de bague");
    }
}

#[test]
fn ligne_automatique_complete_autobuyer_vers_guichet() {
    let mut sim = Sim::new([42u8; 32], 1_000_000);
    let start = sim.eco.money;
    remplir_stock(&mut sim);
    // Triche de test : le test veut bâtir la ligne complète d'emblée
    // (en jeu, il faudrait d'abord vendre à la main — c'est le design).
    sim.eco.earn(10_000.0);

    // Ligne : AutoBuyer -> belt -> belt -> Dépaqueteur -> belt -> Guichet.
    sim.place_machine(-2, 0, MachineKind::AutoBuyer, Dir::East).unwrap();
    sim.place_belt(-1, 0, Dir::East).unwrap();
    sim.place_belt(0, 0, Dir::East).unwrap();
    sim.place_machine(1, 0, MachineKind::Unpacker, Dir::East).unwrap();
    sim.place_belt(2, 0, Dir::East).unwrap();
    sim.place_machine(3, 0, MachineKind::Seller, Dir::East).unwrap();

    // 3 minutes de simulation à 60 Hz.
    for _ in 0..(60 * 180) {
        sim.tick(1.0 / 60.0);
    }

    // L'auto-acheteur a tourné, le dépaqueteur a ouvert, le guichet a vendu.
    assert!(sim.eco.packages_bought >= 20, "auto-acheteur trop lent: {}", sim.eco.packages_bought);
    let unpacker = sim.machines.iter().find(|m| m.kind == MachineKind::Unpacker).unwrap();
    let seller = sim.machines.iter().find(|m| m.kind == MachineKind::Seller).unwrap();
    assert!(unpacker.processed >= 15, "dépaqueteur: {}", unpacker.processed);
    assert!(seller.processed >= 10, "guichet: {}", seller.processed);

    // La fabrique tourne sans blackout et garde du capital.
    assert!(!sim.blackout);
    assert!(sim.eco.money > start * 0.5, "argent: {}", sim.eco.money);
    assert_eq!(sim.eco.packages_left, 1_000_000 - sim.eco.packages_bought);
}

#[test]
fn incinerateur_produit_de_l_energie_et_brule_les_cartons() {
    let mut sim = Sim::new([99u8; 32], 1_000_000);
    remplir_stock(&mut sim);
    // Ligne : AutoBuyer -> belt -> Dépaqueteur -> belt -> Incinérateur.
    sim.place_machine(-2, 0, MachineKind::AutoBuyer, Dir::East).unwrap();
    sim.place_belt(-1, 0, Dir::East).unwrap();
    sim.place_belt(0, 0, Dir::East).unwrap();
    sim.place_machine(1, 0, MachineKind::Unpacker, Dir::East).unwrap();
    sim.place_belt(2, 0, Dir::East).unwrap();
    sim.place_machine(3, 0, MachineKind::Incinerator, Dir::East).unwrap();

    for _ in 0..(60 * 120) {
        sim.tick(1.0 / 60.0);
    }
    // Des cartons sont partis en fumée -> énergie produite.
    assert!(sim.eco.energy_burned_kwh > 0.0, "aucune énergie produite");
    let incin = sim.machines.iter().find(|m| m.kind == MachineKind::Incinerator).unwrap();
    assert!(incin.processed > 0);
}

#[test]
fn trieur_repartit_sur_ses_deux_sorties() {
    let mut sim = Sim::new([11u8; 32], 1_000_000);
    remplir_stock(&mut sim);
    // AutoBuyer -> belt -> belt -> Splitter(dir Est) -> [Nord | Sud] -> 2 incinérateurs
    sim.place_machine(-3, 0, MachineKind::AutoBuyer, Dir::East).unwrap();
    sim.place_belt(-2, 0, Dir::East).unwrap();
    sim.place_belt(-1, 0, Dir::East).unwrap();
    sim.place_machine(0, 0, MachineKind::Splitter, Dir::East).unwrap();
    // Sortie droite du splitter (dir Est) = Sud -> (0,1)
    sim.place_belt(0, 1, Dir::East).unwrap();
    sim.place_machine(1, 1, MachineKind::Incinerator, Dir::East).unwrap();
    // Sortie gauche = Nord -> (0,-1)
    sim.place_belt(0, -1, Dir::East).unwrap();
    sim.place_machine(1, -1, MachineKind::Incinerator, Dir::East).unwrap();

    for _ in 0..(60 * 180) {
        sim.tick(1.0 / 60.0);
    }
    let incins: Vec<_> = sim.machines.iter().filter(|m| m.kind == MachineKind::Incinerator).collect();
    assert_eq!(incins.len(), 2);
    let a = incins[0].processed;
    let b = incins[1].processed;
    assert!(a > 0 && b > 0, "trieur déséquilibré: {a} vs {b}");
    // Alternance stricte : |a - b| <= 1.
    assert!((a as i64 - b as i64).abs() <= 1, "alternance cassée: {a} vs {b}");
}

#[test]
fn blackout_gèle_la_fabrique_et_la_vente_manuelle_repare() {
    let mut sim = Sim::new([5u8; 32], 1_000_000);
    remplir_stock(&mut sim);
    sim.place_machine(-2, 0, MachineKind::AutoBuyer, Dir::East).unwrap();
    sim.place_belt(-1, 0, Dir::East).unwrap();
    sim.place_belt(0, 0, Dir::East).unwrap();
    sim.place_machine(1, 0, MachineKind::Unpacker, Dir::East).unwrap();
    sim.place_belt(2, 0, Dir::East).unwrap();
    sim.place_machine(3, 0, MachineKind::Seller, Dir::East).unwrap();

    // Ruine volontaire du joueur.
    sim.eco.money = 0.01;
    for _ in 0..(60 * 60) {
        sim.tick(1.0 / 60.0);
    }
    assert!(sim.blackout, "devrait être en blackout à 0.01 €");
    let bought_at_blackout = sim.eco.packages_bought;

    // Pendant le blackout, plus aucun achat automatique.
    for _ in 0..300 {
        sim.tick(1.0 / 60.0);
    }
    assert_eq!(sim.eco.packages_bought, bought_at_blackout);

    // Vente manuelle de secours -> courant rétabli.
    sim.eco.earn(100.0);
    assert!(sim.manual_buy_package());
    let last = sim.floor.len() - 1;
    sim.manual_open(last).unwrap();
    while sim
        .floor
        .get(last)
        .and_then(|p| p.items.as_ref())
        .is_some_and(|v| !v.is_empty())
    {
        sim.manual_sell_item(last, 0);
    }
    for _ in 0..600 {
        sim.tick(1.0 / 60.0);
        if !sim.blackout {
            break;
        }
    }
    assert!(!sim.blackout, "le courant devrait être rétabli après vente manuelle");
}

#[test]
fn pool_epuise_apres_achat_de_tout() {
    let mut sim = Sim::new([1u8; 32], 1_000_000);
    sim.eco.money = 1e12;
    for _ in 0..1_000_000 {
        sim.manual_buy_package();
        // On vide la zone sinon FLOOR_PACKAGE_LIMIT bloque.
        sim.floor.clear();
    }
    assert!(sim.eco.pool_empty());
    assert!(!sim.manual_buy_package());
}

#[test]
fn empreinte_seed_stable_pour_reproductibilite() {
    let seed = [0xABu8; 32];
    let hex = seed_fingerprint(&seed);
    assert_eq!(hex.len(), 64);
    // Deux sims de même seed : même bague, mêmes tirages.
    let mut a = Sim::new(seed, 50_000_000);
    let mut b = Sim::new(seed, 50_000_000);
    assert_eq!(a.eco.ring_slot, b.eco.ring_slot);
    let mut ca = Vec::new();
    let mut cb = Vec::new();
    for _ in 0..5 {
        assert!(a.manual_buy_package());
        assert!(b.manual_buy_package());
        let la = a.floor.len() - 1;
        let lb = b.floor.len() - 1;
        ca.push(a.manual_open(la).unwrap());
        cb.push(b.manual_open(lb).unwrap());
    }
    assert_eq!(ca, cb);
}

// ======================================================================
//  v0.3 — Ordinateur (boutique + livraisons), Bras robot, Générateur
// ======================================================================

use h33_core::shop::{ShopItem, ALL_SHOP_ITEMS};

#[test]
fn commande_livre_le_stock_et_le_placement_consomme() {
    let mut sim = Sim::new([11u8; 32], 1_000_000);
    sim.eco.earn(10_000.0);

    // Commande 2 bras robots + 3 tapis à l'ordinateur.
    let arm = ShopItem::Machine(MachineKind::RobotArm);
    let money_before = sim.eco.money;
    sim.order(arm, 2).expect("commande bras robot");
    sim.order(ShopItem::Belt, 3).expect("commande tapis");
    // Payé à la commande : débit exact = 2× bras robot + 3× tapis.
    let debited = 2.0 * MachineKind::RobotArm.spec().cost + 3.0 * balance::BELT_COST_EUR;
    assert!((sim.eco.money - (money_before - debited)).abs() < 1e-6,
        "argent: {} != {}", sim.eco.money, money_before - debited);
    assert!(sim.deliveries.len() >= 2);

    // Pas encore livré : placement impossible.
    assert!(sim.place_machine(0, 0, MachineKind::RobotArm, Dir::East).is_err());
    assert!(sim.stock[arm.stock_index()] == 0);

    // Force la livraison + un tick -> stock, puis placement consomme.
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.stock[arm.stock_index()], 2);
    assert_eq!(sim.stock[ShopItem::Belt.stock_index()], 3);
    assert!(sim.place_machine(0, 0, MachineKind::RobotArm, Dir::East).is_ok());
    assert_eq!(sim.stock[arm.stock_index()], 1);
    assert!(sim.place_belt(1, 0, Dir::East).is_ok());
    // Le placement ne redépense PAS (déjà payé à la commande).
    let money_after_stock = sim.eco.money;
    assert!(sim.place_belt(2, 0, Dir::East).is_ok());
    assert!((sim.eco.money - money_after_stock).abs() < 1e-9);
}

#[test]
fn commande_de_colis_livre_sur_les_palettes_avec_file_d_attente() {
    let mut sim = Sim::new([12u8; 32], 1_000_000);
    sim.eco.earn(1e6);

    // Remplit d'abord la zone (24 slots).
    for _ in 0..balance::FLOOR_PACKAGE_LIMIT {
        assert!(sim.manual_buy_package());
    }
    let before = sim.eco.packages_bought;

    // Commande 10 colis : payés, en attente de slots libres.
    sim.order_packages(10).expect("commande colis");
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    // Zone pleine -> rien n'arrive, tout attend.
    assert_eq!(sim.floor.len(), balance::FLOOR_PACKAGE_LIMIT);
    assert_eq!(sim.pending_packages, 10);

    // On vide le colis 0 : les colis en attente se déversent au fil des ticks.
    // (drain par les OBJETS du colis — une fois vidé il est retiré et le
    // colis suivant, FERMÉ, glisse à l'index 0 : la boucle doit s'arrêter.)
    sim.manual_open(0);
    while sim
        .floor
        .get(0)
        .and_then(|p| p.items.as_ref())
        .is_some_and(|v| !v.is_empty())
    {
        sim.manual_sell_item(0, 0);
    }
    assert!(sim.floor.get(0).is_some(), "il reste 23 colis en zone");
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.pending_packages, 9);
    assert_eq!(sim.eco.packages_bought, before + 1);
}

#[test]
fn le_bras_robot_relie_directement_deux_machines_en_coin() {
    // Dépaqueteur ... Bras robot (rotation à 90°) ... Guichet : la pièce
    // voyage SANS tapis direct. Preuve de l'insertion machine->machine.
    let mut sim = Sim::new([13u8; 32], 1_000_000);
    sim.eco.earn(5000.0);
    sim.order(ShopItem::Machine(MachineKind::Unpacker), 1).unwrap();
    sim.order(ShopItem::Machine(MachineKind::RobotArm), 1).unwrap();
    sim.order(ShopItem::Machine(MachineKind::Seller), 1).unwrap();
    sim.order(ShopItem::Belt, 2).unwrap();
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);

    // Ligne : Dépaqueteur (0,0)→Est, tapis (1,0)→Est, Bras robot (2,0)→Sud,
    // tapis (2,1)→Est, Guichet (3,1).
    assert!(sim.place_machine(0, 0, MachineKind::Unpacker, Dir::East).is_ok());
    assert!(sim.place_belt(1, 0, Dir::East).is_ok());
    assert!(sim.place_machine(2, 0, MachineKind::RobotArm, Dir::South).is_ok());
    assert!(sim.place_belt(2, 1, Dir::East).is_ok());
    assert!(sim.place_machine(3, 1, MachineKind::Seller, Dir::East).is_ok());

    // Une CHAUSSETTE sur le tapis (1,0) -> fin de cellule -> entrée bras robot.
    // (le guichet refuse les colis fermés : on teste avec un objet ouvert)
    sim.belts[0].item = Some(h33_core::grid::CarriedItem {
        kind: ItemKind::Chaussette,
        progress: 0.999,
        contains_ring: false,
    });

    // Assez de ticks pour : tapis->robot (cycle 0,25 s) -> tapis -> guichet.
    for _ in 0..120 {
        sim.tick(1.0 / 60.0);
    }
    let arm = sim.machines.iter().find(|m| m.kind == MachineKind::RobotArm).unwrap();
    assert!(arm.processed >= 1, "le bras robot n'a rien manipulé");
    let seller = sim.machines.iter().find(|m| m.kind == MachineKind::Seller).unwrap();
    assert!(seller.processed >= 1, "le guichet n'a rien reçu du bras robot");
}

#[test]
fn le_bras_robot_fait_transiter_les_colis_et_leur_bague() {
    // Un colis FERMÉ (avec flag bague) traverse un bras robot jusqu'au
    // dépaqueteur : le flag doit arriver intact (l'ouverture le consomme).
    let mut sim = Sim::new([15u8; 32], 1_000_000);
    sim.eco.earn(5000.0);
    sim.order(ShopItem::Machine(MachineKind::RobotArm), 1).unwrap();
    sim.order(ShopItem::Machine(MachineKind::Unpacker), 1).unwrap();
    sim.order(ShopItem::Belt, 2).unwrap();
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);

    // tapis (0,0)→Est -> Bras robot (1,0)→Sud -> tapis (1,1)→Est ->
    // Dépaqueteur (2,1).
    assert!(sim.place_belt(0, 0, Dir::East).is_ok());
    assert!(sim.place_machine(1, 0, MachineKind::RobotArm, Dir::South).is_ok());
    assert!(sim.place_belt(1, 1, Dir::East).is_ok());
    assert!(sim.place_machine(2, 1, MachineKind::Unpacker, Dir::East).is_ok());

    sim.belts[0].item = Some(h33_core::grid::CarriedItem {
        kind: ItemKind::ColisFerme,
        progress: 0.999,
        contains_ring: true, // colis de la bague !
    });

    // Phase 1 : le bras robot manipule le colis -> le flag bague DOIT
    // survivre sur le tapis de sortie (régression v0.3 : il était écrasé).
    for _ in 0..120 {
        sim.tick(1.0 / 60.0);
        if sim.machines.iter().any(|m| m.kind == MachineKind::RobotArm && m.processed >= 1) {
            break;
        }
    }
    let out_belt = sim
        .belts
        .iter()
        .find(|b| b.item.is_some() && b.item.as_ref().unwrap().kind == ItemKind::ColisFerme)
        .expect("le colis a disparu en route");
    assert!(
        out_belt.item.as_ref().unwrap().contains_ring,
        "le flag bague a été PERDU par le bras robot !"
    );

    // Phase 2 : transit complet jusqu'au dépaqueteur (qui l'ouvre).
    for _ in 0..320 {
        sim.tick(1.0 / 60.0);
    }
    let unpacker = sim.machines.iter().find(|m| m.kind == MachineKind::Unpacker).unwrap();
    assert!(unpacker.processed >= 1, "le colis n'est jamais arrivé au dépaqueteur");
}

#[test]
fn le_generateur_fournit_des_kw_gratuits() {
    let mut sim = Sim::new([14u8; 32], 1_000_000);
    sim.eco.earn(5000.0);
    sim.order(ShopItem::Machine(MachineKind::Generator), 2).unwrap();
    sim.order(ShopItem::Belt, 1).unwrap();
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);

    let (demand_before, free_before) = sim.power_summary();
    assert!(free_before < 1.0);

    assert!(sim.place_machine(5, 5, MachineKind::Generator, Dir::South).is_ok());
    assert!(sim.place_machine(6, 5, MachineKind::Generator, Dir::South).is_ok());
    let (demand, free) = sim.power_summary();
    assert!((free - 2.0 * balance::GENERATOR_FREE_KW).abs() < 1e-4);
    // Le générateur consomme quand même son propre balayage.
    assert!(demand >= demand_before + 2.0 * balance::GENERATOR.kw - 1e-4);

    // Un tapis + le générateur : la facture réseau doit rester nulle.
    assert!(sim.place_belt(0, 0, Dir::East).is_ok());
    let money = sim.eco.money;
    for _ in 0..120 {
        sim.tick(1.0 / 60.0);
    }
    // kW gratuits > demande (2 générateurs = 12 kW vs ~1,1 kW) -> 0 € réseau.
    assert!((sim.eco.money - money).abs() < 1e-9, "le réseau a facturé malgré les générateurs");
}

#[test]
fn le_catalogue_couvre_toutes_les_machines() {
    // Le stock est aligné sur le catalogue : index stables, uniques.
    let mut seen = std::collections::HashSet::new();
    for item in ALL_SHOP_ITEMS {
        assert!(seen.insert(item.stock_index()), "doublon d'index stock");
    }
    assert_eq!(ALL_SHOP_ITEMS.len(), 9);
}

// ======================================================================
//  Livraisons de colis commandées (fix v0.3.10 : plus de double débit)
// ======================================================================

#[test]
fn colis_payes_davance_arrivent_meme_avec_solde_vide() {
    let mut sim = Sim::new([21u8; 32], 1_000_000);
    // Portefeuille = EXACTEMENT de quoi payer la commande (et PAS assez
    // pour un re-débit à la livraison — l'ancien bug donnait 0 colis).
    sim.eco.money = balance::PACKAGE_PRICE_EUR * 3.0 + 1.0;
    assert!(sim.order_packages(3).is_ok());
    assert!((sim.eco.money - 1.0).abs() < 1e-6, "le paiement d'avance doit débiter 3 colis");
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.floor.len(), 3, "les colis PAYÉS doivent être livrés");
    // Aucun re-débit à la livraison : le solde n'a pas bougé.
    assert!((sim.eco.money - 1.0).abs() < 1e-6);
}

#[test]
fn deux_commandes_daffilee_livrent_toutes_les_deux() {
    let mut sim = Sim::new([22u8; 32], 1_000_000);
    sim.eco.money = balance::PACKAGE_PRICE_EUR * 30.0; // 360 €
    assert!(sim.order_packages(10).is_ok());           // -120
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.floor.len(), 10, "1re livraison");
    assert!(sim.order_packages(10).is_ok());           // -120 (2e commande)
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.floor.len(), 20, "la 2e commande doit arriver AUSSI");
    // Débit total : exactement 240 € — pas un centime de plus.
    assert!((sim.eco.money - balance::PACKAGE_PRICE_EUR * 10.0).abs() < 1e-6);
}

#[test]
fn colis_en_attente_livres_quand_la_place_se_libere() {
    let mut sim = Sim::new([23u8; 32], 1_000_000);
    sim.eco.money = balance::PACKAGE_PRICE_EUR * 100.0;
    // Zone de palettes PLEINE (achats dock, payés une fois).
    for _ in 0..balance::FLOOR_PACKAGE_LIMIT {
        assert!(sim.manual_buy_package());
    }
    let money_full = sim.eco.money;
    assert!(sim.order_packages(4).is_ok());
    let money_ordered = sim.eco.money;
    assert!((money_ordered - (money_full - balance::PACKAGE_PRICE_EUR * 4.0)).abs() < 1e-6);
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.floor.len(), balance::FLOOR_PACKAGE_LIMIT);
    assert_eq!(sim.pending_packages, 4, "payés, en attente d'un slot");
    // Libère un slot : le colis en attente arrive SANS re-débit.
    sim.floor.pop();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.pending_packages, 3);
    assert_eq!(sim.floor.len(), balance::FLOOR_PACKAGE_LIMIT);
    assert!((sim.eco.money - money_ordered).abs() < 1e-6, "pas de re-débit sur l'attente");
}

#[test]
fn pool_epuise_avant_livraison_rembourse_les_colis_payes() {
    let mut sim = Sim::new([24u8; 32], 1_000_000);
    sim.eco.money = balance::PACKAGE_PRICE_EUR * 5.0;
    assert!(sim.order_packages(4).is_ok());
    sim.eco.packages_left = 0; // le fournisseur s'assèche entre-temps
    sim.debug_force_deliveries();
    sim.tick(1.0 / 60.0);
    assert_eq!(sim.pending_packages, 0, "pas de colis fantôme en attente");
    assert!((sim.eco.money - balance::PACKAGE_PRICE_EUR * 5.0).abs() < 1e-6, "remboursement intégral");
}

#[test]
fn commande_plafonnee_au_pool_restant() {
    let mut sim = Sim::new([25u8; 32], 1_000_000);
    sim.eco.money = 1_000_000.0;
    sim.eco.packages_left = 3;
    assert!(sim.order_packages(10).is_ok(), "la commande est réduite au pool, pas refusée");
    // Seuls 3 colis sont facturés (36 €), pas 10 (120 €).
    assert!((sim.eco.money - (1_000_000.0 - balance::PACKAGE_PRICE_EUR * 3.0)).abs() < 1e-6);
}
