//! ============================================================
//!  RNG à deux étages — conformité au cahier des charges
//! ============================================================
//!  Étage 1 — TRNG (UNE SEULE FOIS, à la création de la partie) :
//!      la seed maîtresse est tirée de l'API OS native via la crate
//!      `getrandom` :
//!        - Linux/Android : getrandom(2), fallback /dev/urandom,
//!        - macOS : getentropy(2),
//!        - Windows : BCryptGenRandom (RDRAND en aval du pool kernel).
//!      On y mélange en PLUS de l'entropie locale gratuite (jitter
//!      d'horloge haute-résolution + ASLR), au cas où le pool OS serait
//!      neuf après un boot à froid. Aucun réseau impliqué.
//!
//!  Étage 2 — PRNG de GAMEPLAY (tous les tirages de la partie) :
//!      ChaCha8 (fonction de hachage à permutation officielle, 8 tours)
//!      seedé par la seed maîtresse. Rapide (~1 ns/tirage), sans biais,
//!      et NON prédictible de l'extérieur puisque la seed vient du TRNG.
//!      Étant déterministe au sein d'une partie, il est reproductible
//!      pour les tests et compatible serveur autoritaire (multijoueur).
//!
//!  ⚠️  Aucun tirage de gameplay n'appelle `getrandom` (trop lent :
//!      un appel système par tirage à 60 Hz sur des milliers d'objets
//!      serait un goulot). Le TRNG ne sert QU'À LA SEED. C'est voulu.

use std::ops::Range;

/// Récupère de l'entropie native de l'OS (TRNG système, hors-ligne).
fn os_entropy(buf: &mut [u8]) {
    // getrandom 0.3 : `fill` route vers getrandom(2)/getentropy/BCryptGenRandom.
    getrandom::fill(buf).expect("impossible de lire l'entropie système (getrandom)");
}

/// Génère la seed maîtresse d'une partie : TRNG OS + brassage jitter local.
pub fn generate_master_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    os_entropy(&mut seed);

    // --- Entropie complémentaire (jitter) -------------------------------
    // Mesure la latence d'horloge à haute fréquence : le scheduler et le
    // matériel rendent ces valeurs imprévisibles. Mélange XOR sans prétendre
    // remplacer le TRNG OS — c'est un filet de sécurité.
    let mut jitter = 0u64;
    for _ in 0..16 {
        let t0 = std::time::Instant::now();
        // Petite boucle opaque : le temps de mesure varie (cache, préemption).
        let mut acc = 0u64;
        for i in 0..64 {
            acc = acc.wrapping_mul(0x1000193).wrapping_add(i as u64);
        }
        std::hint::black_box(acc);
        jitter = jitter.rotate_left(7) ^ t0.elapsed().as_nanos() as u64;
    }
    let aslr = std::hint::black_box(&jitter) as *const _ as u64;
    let mix: [u8; 8] = (jitter ^ aslr.rotate_left(31)).to_le_bytes();
    for (i, b) in mix.iter().enumerate() {
        seed[i] ^= b;
        seed[i + 16] ^= b.rotate_left(3);
    }
    seed
}

/// Empreinte hexadécimale lisible de la seed (affichée dans le menu,
/// permet de reproduire une partie à l'identique avec `--seed <hex>`).
pub fn seed_fingerprint(seed: &[u8; 32]) -> String {
    seed.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse une empreinte hexadécimale (64 chars) en seed.
pub fn parse_seed(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(out)
}

// ======================================================================
//  ChaCha8 — PRNG de gameplay
// ======================================================================

/// PRNG ChaCha8. ~90 lignes, zéro dépendance, sans biais.
/// (Même famille que ChaCha20 du CSPRNG de Linux, réduite à 8 tours :
///  très largement suffisant pour du gameplay, 2x plus rapide.)
pub struct GameRng {
    key: [u32; 8],
    nonce: [u32; 2],
    counter: u64,
    /// Bloc courant de 16 mots (64 octets) déjà généré.
    block: [u32; 16],
    /// Position dans `block` (0..16) — prochain mot à consommer.
    idx: usize,
}

impl GameRng {
    /// Construit le PRNG depuis la seed maîtresse TRNG.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let mut key = [0u32; 8];
        for (i, word) in key.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                seed[i * 4],
                seed[i * 4 + 1],
                seed[i * 4 + 2],
                seed[i * 4 + 3],
            ]);
        }
        // Nonce dérivé de la moitié haute de la seed.
        let nonce = [key[4] ^ key[6], key[5] ^ key[7]];
        let mut rng = Self { key, nonce, counter: 0, block: [0; 16], idx: 16 };
        rng.refill();
        rng
    }

    /// Re-seed (utilisé par le serveur en multijoueur pour isoler des flux).
    pub fn reseed(&mut self, seed: [u8; 32]) {
        *self = Self::from_seed(seed);
    }

    /// Génère le prochain bloc ChaCha8 (état -> permutation -> add state).
    fn refill(&mut self) {
        // "expand 32-byte k" — constantes officielles ChaCha.
        const CONST: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];
        let mut st = [0u32; 16];
        st[0..4].copy_from_slice(&CONST);
        st[4..12].copy_from_slice(&self.key);
        st[12] = self.counter as u32;
        st[13] = (self.counter >> 32) as u32;
        st[14] = self.nonce[0];
        st[15] = self.nonce[1];

        let mut x = st;
        // 8 tours = 4 double-tours (colonne + diagonale).
        for _ in 0..4 {
            quarter_round(&mut x, 0, 4, 8, 12);
            quarter_round(&mut x, 1, 5, 9, 13);
            quarter_round(&mut x, 2, 6, 10, 14);
            quarter_round(&mut x, 3, 7, 11, 15);
            quarter_round(&mut x, 0, 5, 10, 15);
            quarter_round(&mut x, 1, 6, 11, 12);
            quarter_round(&mut x, 2, 7, 8, 13);
            quarter_round(&mut x, 3, 4, 9, 14);
        }
        for i in 0..16 {
            self.block[i] = x[i].wrapping_add(st[i]);
        }
        self.counter = self.counter.wrapping_add(1);
        self.idx = 0;
    }

    /// Prochain u64 uniforme (consomme deux mots du bloc courant).
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        if self.idx >= 16 {
            self.refill();
        }
        let lo = self.block[self.idx] as u64;
        let hi = self.block[self.idx + 1] as u64;
        self.idx += 2;
        if self.idx >= 16 {
            self.refill();
        }
        lo | (hi << 32)
    }

    /// Entier uniforme dans `range` — échantillonnage par rejet (SANS biais
    /// modulo). Ex : `rng.range_u64(0..total_packages)`.
    pub fn range_u64(&mut self, range: Range<u64>) -> u64 {
        let width = range.end.saturating_sub(range.start);
        debug_assert!(width > 0, "range vide");
        if width == 0 {
            return range.start;
        }
        let limit = u64::MAX - (u64::MAX % width);
        loop {
            let x = self.next_u64();
            if x < limit {
                return range.start + (x % width);
            }
        }
    }

    /// Flottant uniforme dans [0, 1).
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Vrai avec probabilité `p` ∈ [0, 1].
    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// Tirage pondéré : renvoie l'index de l'entrée choisie.
    /// Ex : `PACKAGE_ROLL_TABLE[rng.pick_weighted(&WEIGHTS)]`.
    pub fn pick_weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        debug_assert!(total > 0, "poids nuls");
        let mut target = self.range_u64(0..total as u64) as u32;
        for (i, w) in weights.iter().enumerate() {
            if target < *w {
                return i;
            }
            target -= *w;
        }
        weights.len() - 1 // inatteignable en pratique
    }
}

/// Cœur ChaCha : une demi-tour (Add-Rotate-Xor), se vectorise très bien.
#[inline(always)]
fn quarter_round(x: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    x[a] = x[a].wrapping_add(x[b]); x[d] ^= x[a]; x[d] = x[d].rotate_left(16);
    x[c] = x[c].wrapping_add(x[d]); x[b] ^= x[c]; x[b] = x[b].rotate_left(12);
    x[a] = x[a].wrapping_add(x[b]); x[d] ^= x[a]; x[d] = x[d].rotate_left(8);
    x[c] = x[c].wrapping_add(x[d]); x[b] ^= x[c]; x[b] = x[b].rotate_left(7);
}

// ======================================================================
//  Tests unitaires
// ======================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_fingerprint_roundtrip() {
        let s = generate_master_seed();
        let hex = seed_fingerprint(&s);
        assert_eq!(hex.len(), 64);
        assert_eq!(parse_seed(&hex).unwrap(), s);
    }

    #[test]
    fn chacha_reproductible_et_distinct() {
        let seed = [42u8; 32];
        let mut a = GameRng::from_seed(seed);
        let mut b = GameRng::from_seed(seed);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = GameRng::from_seed([43u8; 32]);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn ranges_dans_bornes_sans_biais_grossier() {
        let mut rng = GameRng::from_seed([7u8; 32]);
        let mut bins = [0u32; 5];
        for _ in 0..50_000 {
            let v = rng.range_u64(0..5);
            assert!(v < 5);
            bins[v as usize] += 1;
        }
        // Uniformité grossière : aucune case sous 60 % de la moyenne.
        let mean = 50_000.0 / 5.0;
        for b in bins {
            assert!(b as f64 > mean * 0.6, "biais détecté: {bins:?}");
        }
    }

    #[test]
    fn f32_dans_intervalle() {
        let mut rng = GameRng::from_seed([9u8; 32]);
        for _ in 0..10_000 {
            let f = rng.f32();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
