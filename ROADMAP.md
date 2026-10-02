# ROADMAP — HANGAR 33

Découpage incrémental. Chaque étape est jouable à la fin de l'étape.
(V0.1 est faite — voir git/README.)

## v0.1 — Vertical Slice ✅ (fait)

- [x] Workspace Rust : `h33-core` (logique pure) / `h33-render` (wgpu) / `h33-app` (winit+egui)
- [x] TRNG (seed, `getrandom` + jitter) → PRNG ChaCha8 gameplay sans biais
- [x] Menu : choix du total de colis 1 M → 500 M (slider log), empreinte seed
- [x] Boucle manuelle complète : acheter → ouvrir → vendre / brûler
- [x] Règles bague : cachée dans 1 colis / vente = −25 % + recache dans les
      colis NON achetés / incinération = recache / sécurisation = victoire
- [x] Machines : Auto-Acheteur, Dépaqueteur, Guichet, Incinérateur, Trieur 2 voies,
      Scanner à bague ; tapis directionnels + embouteillages (1 slot/cellule)
- [x] Électricité : réseau 0,15 €/kWh, kW gratuits via incinérateurs, blackout
      (gel total sauf vente manuelle, relance par revenu)
- [x] Rendu : 1 draw call instancié, sol grille procédurale, fog, picking cellule,
      ghost de placement, bague pulsante dorée
- [x] HUD + toasts + panneau machine (réglage période Auto-Acheteur) + aide
- [x] Tests : 20+ (sim, économie, bague, lignes, blackout, rendu offscreen)
- [x] CI-like : smoke-test scripté sous Xvfb + lavapipe → screenshot + exit 0

## v0.2 — « Ça se joue longtemps » (2-3 semaines)

- [ ] **Sauvegarde** (serde + JSON local) — reprise d'état, AUCUN gain offline
      (clamp du delta-time au retour)
- [ ] **Upgrades** : vitesse tapis v2, dépaqueteur ×2 slots, fichier de
      recherche (arbre à 3 branches : Logistique / Énergie / Chance)
- [ ] **Réemballeur** : N objets sans valeur → colis « lot » vendu plus cher
      (brique de la boucle en gros)
- [ ] **Trieur filtrable** : règle par rareté/type (UI), pour protéger
      l'incinérateur proprement (le trieur v0.1 ne route qu'alternativement)
- [ ] **VFX ECS** : particules de déballage, vol d'objets vers le guichet,
      fumée d'incinérateur (le mini-ECS est prêt)
- [ ] **Sons** (kira) : pop de colis, ka-ching, alarme bague, grésillement électrique
- [ ] **Stats de partie** : histogramme des raretés vendues, €/min, facture réseau

## v0.3 — « Pipeline Blender MCP » (FAIT — voir README « Assets Blender »)

- [x] Serveur MCP Blender (mcp-for-blender, socket JSON 9876) : hangar réel
      (dalle, murs nervurés, charpente, lanterneaux, lampes, racks, palettes,
      barils, bureau) + 6 machines + colis générés via `execute_code`
- [x] Format asset : GLB embarqués (`include_bytes!`), loader maison
      (`gltf_asset.rs`), 1 mesh fusionné par fichier, lots instanciés
- [x] Vue première personne (pointer lock, ZQSD physique, sprint, crosshair)
      remplaçant l'orbite (conservée pour debug/tests)
- [ ] Matériaux stylisés (flat + bordures cel-shading légères)
- [ ] Ombres portées douces (1 pass shadow map sur le soleil)
- [ ] Collisions joueur/machines (v1 : bornes murs uniquement)

## v0.3.5 — « L'Ordinateur » ✅ (fait)

- [x] **HANGAR-OS 33** : ordinateur 3D modélisé (Bureau CO_ via Blender MCP)
      au sud-ouest ; interaction **E** à proximité (rayon 2,6 m) ; l'écran
      clignote quand des livraisons sont en route
- [x] **Boutique à livraisons** (style Find The Needle) : payé À LA COMMANDE,
      livraison différée (~8 s machines/tapis, colis selon le volume) ;
      le placement consomme le stock livré ; démolition rembourse 50 %
- [x] **Terminal** (`help/status/stock/order…/clear`) réellement branché à la
      sim (`terminal_exec` réutilisable UI + smoke-test)
- [x] **Code secret** : lance une VRAIE VM hôte — QEMU/KVM (repli TCG) Linux,
      Hyper-V (HANGAR33-VM + VMConnect) Windows, UTM macOS (vm.rs)
- [x] **Bras robot** (180 €) : manipulateur 1×1, insertion machine->machine,
      angle 90° ; flag bague préservé (test de régression dédié) ; les
      sorties bloquées retiennent l'objet au lieu de l'écraser
- [x] **Générateur** (400 €) : 6 kW gratuits continus (hors-réseau)
- [x] 3 nouveaux assets Blender MCP : computer.glb, machine_robotarm.glb,
      machine_generator.glb + rendus Cycles de contrôle
- [x] Smoke-test étendu : chaîne AutoBuyer→Dépaqueteur→**Bras**→Guichet
      productive, commandes terminal réelles, 2 captures (FP + terminal)

## v0.3.7 — « HANGAR-OS 11 » ✅ (fait)

- [x] **Faux bureau Windows 11** dans l'ordinateur du hangar : fond d'écran
      « bloom » (dégradé + auréoles), icônes Ce PC / Corbeille (easter eggs),
      barre des tâches centrée (Démarrer + apps + horloge de hangar),
      menu Démarrer (recherche, tuiles épinglées, Éteindre) et fenêtres
      d'apps superposables (Boutique / Colis / Terminal)
- [x] **Fix livraisons invisibles** : la zone de palettes était À L'EXTÉRIEUR
      des murs (z = half + 3) — déplacée DANS le hangar (mur sud,
      `balance::DOCK_ZONE_Z`), clamp joueur resserré, marqueur doré pulsant
      au-dessus du dock, compte à rebours de livraison dans le HUD,
      toasts localisés (« mur SUD »)
- [x] **VM bootable** (code secret) : ISO Windows auto-détectée
      (Téléchargements/Bureau/Documents/C:\ISO) et branchée en DVD de boot
      Hyper-V (2 Go RAM, FirstBootDevice=DVD) / `-cdrom -boot order=d`
      QEMU ; firmware **TianoCore (OVMF)** détecté sur Linux (pflash +
      NVRAM copiée en writable, repli `-bios`, repli SeaBIOS) ; tests
      unitaires (firmware_args, préférence ISO, dossiers de recherche)
- [x] Smoke-test revalidé : dock F-interactif à la nouvelle position,
      bureau HANGAR-OS 11 capturé en screenshot

## v0.4 — « Multijoueur coop » (3-4 semaines, l'architecture est déjà là)

- [ ] Serveur dédié headless = `h33-core` + QUIC/TCP (quinn), commandes
      joueurs sérialisées, tick 60 Hz autoritaire
- [ ] Client : prédiction locale des tapis (mouvement purement continu =
      interpolation triviale), reconciliation sur les événements discrets
- [ ] Le TRNG reste local au serveur ; les tirages (loot) sont répliqués par
      événement (`PackageOpened { contents }`) — jamais prédits côté client
- [ ] 2-4 joueurs dans le même hangar, permissions de construction par zone

## v0.5 — « Devenir usine » (le long terme)

- [ ] Étages/niveaux du hangar, zones à débloquer
- [ ] Marché dynamique : les prix baissent quand tu inondes le marché
      d'un objet (pousse à la diversification)
- [ ] Événements fournisseur : « lot surprise », colis maudits, inspection douane
- [ ] Modes : Zen (sans blackout), Hardcore (bague perdue = fin de partie),
      Course (2 hangars en parallèle)
- [ ] Steamworks : achievements « Vendre la bague 3 fois » (le joueur cupide),
      « 500 M colis », « blackout maîtrisé »

---

### Arbitrages à trancher avec le game designer (ouverts)

1. **Incinérer la bague** : actuellement re-cache gratuite + malus de temps.
   Proposals : (a) amende d'assurance fixe, (b) le pool perd 10 000 colis
   (la bague « se cache plus profondément »), (c) statu quo.
2. **Valeur de revente à l'aveugle** d'un colis fermé : 7 € actuellement
   (58 % du prix). C'est le seul « cash-back » anti-ruine — trop ou pas assez ?
3. **Économie** : EV/colis ≈ 18 € pour 12 € l'achat (marge ~50 %). La marge
   doit-elle fondre quand le prix du marché baisse (v0.5) ou via inflation
   des coûts de machines au nb posé ?
4. **Scanner à bague** : 800 € est-il assez cher pour que la première bague
   vendue soit un rite de passage (tuto par l'échec) ?
