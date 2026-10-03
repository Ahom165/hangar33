//! Easter egg HANGAR-OS : le code secret du terminal lance une VRAIE
//! machine virtuelle sur la machine hôte — parce que c'est drôle.
//!
//! Selon l'OS :
//!  - Linux/BSD : QEMU avec KVM (`-accel kvm`), repli TCG si /dev/kvm est
//!    absent. FIRMWARE : si OVMF (TianoCore/EDK2) est installé, la VM
//!    démarre sur le VRAI Boot Manager UEFI ; BONUS : si une ISO
//!    (Windows/Linux) traîne dans Téléchargements, elle est branchée en
//!    CD-ROM et la VM boote DESSUS (2 Go de RAM alloués). Sinon SeaBIOS.
//!  - Windows   : Hyper-V via un script PowerShell autonome (écrit dans
//!    %TEMP%) qui se réélève en UAC si besoin, CRÉE "HANGAR33-VM" si elle
//!    n'existe pas, branche la première ISO Windows trouvée (Téléchargements,
//!    Bureau, Documents, C:\ISO) comme DVD de boot (2 Go de RAM), la démarre
//!    et ouvre la console VMConnect. Sans ISO : la VM démarre quand même
//!    (PXE/UEFI) et le message explique comment fournir une ISO.
//!    AUTO-RÉPARATION : si Start-VM échoue (ex. fichier d'état .vmgs jamais
//!    créé sur une VM à moitié provisionnée — bug Hyper-V/Win11), le script
//!    redémarre le service vmms, SUPPRIME la VM, la recrée proprement avec
//!    tout branché avant le premier boot, et retente une seconde fois.
//!  - macOS     : UTM (front-end Virtualization.framework) s'il est là.
//!
//! Aucune sortie n'est attendue du process : on spawn détaché et on
//! rend la main immédiatement (le jeu ne se fige jamais sur une blague).

#[cfg(unix)]
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Nom donné à la VM pour la retrouver dans l'hyperviseur.
pub const VM_NAME: &str = "HANGAR33-VM";

/// Lance la VM adaptée à l'OS hôte. Renvoie une description de ce qui
/// a été fait (affichée dans le terminal HANGAR-OS) ou une erreur
/// actionnable.
pub fn launch_real_vm() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        hyper_v()
    }
    #[cfg(target_os = "macos")]
    {
        utm()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        qemu_kvm()
    }
}

// ======================================================================
//  Détection d'ISO (partagé QEMU/UTM côté Unix ; Hyper-V a son scan PS1)
// ======================================================================

/// Dossiers où chercher une ISO bootable (le joueur a DÉJÀ une ISO Windows
/// quelque part — on ne va quand même pas télécharger 5 Go pour une blague).
#[cfg(unix)]
fn iso_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        for d in ["Téléchargements", "Downloads", "iso", "ISO", "isos", "Bureau", "Desktop"] {
            dirs.push(home.join(d));
        }
        dirs.push(home);
    }
    dirs.push(PathBuf::from("/var/lib/libvirt/images"));
    dirs
}

/// Parmi des chemins d'ISO, préfère une Windows (le rêve), sinon la première.
/// (pur — testable sans disque ; côté Windows, c'est le script Hyper-V qui
/// fait sa propre préférence en PowerShell)
#[cfg(unix)]
fn prefer_windows_iso(paths: &[PathBuf]) -> Option<PathBuf> {
    let win = paths.iter().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().contains("win"))
    });
    win.or_else(|| paths.first()).cloned()
}

#[cfg(unix)]
fn find_host_iso() -> Option<PathBuf> {
    let mut found = Vec::new();
    for d in iso_search_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let is_iso = p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("iso"));
            if is_iso {
                found.push(p);
            }
        }
    }
    prefer_windows_iso(&found)
}

// ======================================================================
//  Linux / BSD : QEMU + KVM (repli TCG) + TianoCore/OVMF + ISO
// ======================================================================

#[cfg(all(unix, not(target_os = "macos")))]
pub fn qemu_args() -> Vec<String> {
    vec![
        "-name".into(), format!("{VM_NAME} (HANGAR 33)"),
        "-m".into(), "256".into(),
        // Chaîne de backends : KVM si dispo, sinon TCG (émulation logicielle).
        "-accel".into(), "kvm".into(),
        "-accel".into(), "tcg".into(),
        // Une fenêtre : voir la VM booter EST la blague.
        "-display".into(), "sdl".into(),
        "-nic".into(), "none".into(),
    ]
}

/// Firmware détecté pour QEMU.
#[cfg(all(unix, not(target_os = "macos")))]
enum Fw {
    /// OVMF split : code en lecture seule + variables (NVRAM) en écriture.
    Pflash { code: PathBuf, vars: Option<PathBuf> },
    /// OVMF combiné (image BIOS unique).
    Bios(PathBuf),
}

/// Cherche un firmware OVMF (TianoCore/EDK2) aux emplacements standards des
/// principales distributions. Rien n'est téléchargé : on utilise ce qui est
/// déjà là (paquet `ovmf` sur Debian/Ubuntu/Arch/Fedora).
#[cfg(all(unix, not(target_os = "macos")))]
fn ovmf_firmware() -> Option<Fw> {
    const CODE_VARS: &[(&str, &str)] = &[
        ("/usr/share/OVMF/OVMF_CODE_4M.fd", "/usr/share/OVMF/OVMF_VARS_4M.fd"),
        ("/usr/share/OVMF/OVMF_CODE.secboot.fd", "/usr/share/OVMF/OVMF_VARS.secboot.fd"),
        ("/usr/share/OVMF/OVMF_CODE.fd", "/usr/share/OVMF/OVMF_VARS.fd"),
        ("/usr/share/edk2-ovmf/x64/OVMF_CODE.fd", "/usr/share/edk2-ovmf/x64/OVMF_VARS.fd"),
        ("/usr/share/edk2/ovmf/OVMF_CODE.fd", "/usr/share/edk2/ovmf/OVMF_VARS.fd"),
        ("/usr/share/qemu/OVMF_CODE.fd", "/usr/share/qemu/OVMF_VARS.fd"),
    ];
    for (c, v) in CODE_VARS {
        let code = PathBuf::from(c);
        if code.is_file() {
            let vars = PathBuf::from(v);
            let vars = if vars.is_file() { Some(vars) } else { None };
            return Some(Fw::Pflash { code, vars });
        }
    }
    for b in ["/usr/share/ovmf/OVMF.fd", "/usr/share/OVMF/OVMF.fd"] {
        let p = PathBuf::from(b);
        if p.is_file() {
            return Some(Fw::Bios(p));
        }
    }
    None
}

/// Arguments QEMU pour brancher un firmware pflash (code en RO + NVRAM).
/// (pur — testable ; la copie des vars est faite par l'appelant)
#[cfg(all(unix, not(target_os = "macos")))]
fn firmware_args(code: &std::path::Path, vars: Option<&std::path::Path>) -> Vec<String> {
    let mut v = vec![
        "-drive".to_string(),
        format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
    ];
    if let Some(vars) = vars {
        v.push("-drive".to_string());
        v.push(format!("if=pflash,format=raw,file={}", vars.display()));
    }
    v
}

#[cfg(all(unix, not(target_os = "macos")))]
fn qemu_kvm() -> Result<String, String> {
    let iso = find_host_iso();
    let fw = ovmf_firmware();

    let mut args = qemu_args();
    if let Some(iso) = &iso {
        // Une installatrice (Windows notamment) veut de la RAM : 2 Go.
        if let Some(i) = args.iter().position(|a| a == "-m") {
            args[i + 1] = "2048".into();
        }
        args.push("-cdrom".into());
        args.push(iso.to_string_lossy().into_owned());
        args.push("-boot".into());
        args.push("order=d".into());
    }
    match &fw {
        Some(Fw::Pflash { code, vars }) => {
            // Les variables NVRAM doivent être accessibles en écriture :
            // copie du template dans %TEMP% (OVMF y écrit au boot).
            let vars_copy = vars.as_ref().and_then(|v| {
                let tmp = std::env::temp_dir().join("hangar33_ovmf_vars.fd");
                std::fs::copy(v, &tmp).ok().map(|_| tmp)
            });
            args.extend(firmware_args(code, vars_copy.as_deref()));
        }
        Some(Fw::Bios(path)) => {
            args.push("-bios".into());
            args.push(path.to_string_lossy().into_owned());
        }
        None => {}
    }

    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(_child) => {
            let kvm = std::path::Path::new("/dev/kvm").exists();
            let accel = if kvm { "KVM (accélération matérielle)" } else { "TCG (émulation logicielle, c'est lent)" };
            let iso_name = iso.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("?");
            Ok(match (&iso, &fw) {
                (Some(_), _) => format!(
                    "VM lancée : QEMU/{accel}, firmware TianoCore (OVMF), boot sur l'ISO « {iso_name} ». \
                     Si c'est une ISO Windows : l'installateur démarre pour de vrai."
                ),
                (None, Some(_)) => format!(
                    "VM lancée : QEMU/{accel} sur le firmware TianoCore (OVMF) — le Boot Manager UEFI EDK2 \
                     s'affiche. Aucune ISO trouvée : dépose une ISO Windows dans ~/Téléchargements et \
                     retape le code pour booter dessus."
                ),
                (None, None) => format!(
                    "VM lancée : QEMU/{accel}, SeaBIOS. Astuce : `sudo apt install ovmf` donnera le \
                     firmware TianoCore, et une ISO Windows dans ~/Téléchargements sera bootée automatiquement."
                ),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(
            "QEMU introuvable. Sur Debian/Ubuntu : sudo apt install qemu-system-x86".into(),
        ),
        Err(e) => Err(format!("Impossible de lancer QEMU : {e}")),
    }
}

// ======================================================================
//  Windows : Hyper-V — script autonome + élévation UAC + création VM + ISO
// ======================================================================

#[cfg(target_os = "windows")]
fn hyper_v() -> Result<String, String> {
    // Le script PowerShell (HYPERV_PS1) fait TOUT le travail dans un
    // process détaché : vérif des droits, ré-élévation UAC ("sudo" du
    // terminal), création de la VM si absente, branchement d'une ISO
    // Windows si trouvée, démarrage, VMConnect.
    let script = std::env::temp_dir().join("hangar33_vm.ps1");
    // BOM UTF-8 : sans lui, PowerShell 5.1 lit le fichier en ANSI (accents morts).
    std::fs::write(&script, format!("\u{FEFF}{HYPERV_PS1}"))
        .map_err(|e| format!("écriture du script Hyper-V impossible : {e}"))?;
    Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| format!(
            "séquence Hyper-V lancée — si Windows demande l'autorisation (UAC), ACCEPTE : \
             la VM '{VM_NAME}' est créée si besoin (et RÉPARÉE automatiquement si elle \
             refuse de démarrer), boote sur la première ISO Windows trouvée \
             (Téléchargements, Bureau, Documents, C:\\ISO) sinon sur son firmware, et sa console s'ouvre."
        ))
        .map_err(|e| format!("Impossible de lancer PowerShell : {e}"))
}

/// Script d'installation/lancement de la VM Hyper-V (voir `hyper_v`).
/// tout en une passe : élévation, création, ISO, démarrage, console.
#[cfg(target_os = "windows")]
const HYPERV_PS1: &str = r#"
$ErrorActionPreference = 'Stop'
$vmName = 'HANGAR33-VM'

function Fail([string]$msg) {
  try { Add-Type -AssemblyName System.Windows.Forms } catch {}
  [System.Windows.Forms.MessageBox]::Show($msg, 'HANGAR-OS : Hyper-V', 'OK', 'Error') | Out-Null
  exit 1
}

# Pas admin ? Relance elevée du même script — le "sudo" du terminal HANGAR-OS.
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Start-Process powershell -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File', ('"' + $PSCommandPath + '"'))
  exit 0
}

# ISO bootable ? (préférence pour une ISO Windows — le rêve du joueur)
function Find-Iso {
  $isoDirs = @((Join-Path $env:USERPROFILE 'Downloads'), (Join-Path $env:USERPROFILE 'Téléchargements'), (Join-Path $env:USERPROFILE 'Desktop'), (Join-Path $env:USERPROFILE 'Documents'), 'C:\ISO')
  $isos = @()
  foreach ($d in $isoDirs) {
    if (Test-Path $d) { $isos += @(Get-ChildItem -Path $d -Filter *.iso -File -ErrorAction SilentlyContinue) }
  }
  if ($isos.Count -eq 0) { return $null }
  $winIso = $isos | Where-Object { $_.Name -match 'win' } | Select-Object -First 1
  if ($winIso) { return $winIso }
  return ($isos | Select-Object -First 1)
}

# Crée la VM fraîche, AVEC tout branché AVANT le premier boot (l'ordre importe :
# c'est le premier démarrage qui fait écrire à vmms les fichiers d'état).
function New-HangarVm([string]$isoPath) {
  # Gen 2 (UEFI), sans disque : boote sur le DVD branché ci-dessous, sinon PXE.
  New-VM -Name $vmName -Generation 2 -MemoryStartupBytes 536870912 -NoVHD | Out-Null
  Start-Sleep -Milliseconds 800
  if ($isoPath) {
    # 2 Go pour l'installatrice + DVD branché + boot device = DVD.
    Set-VMMemory -VMName $vmName -StartupBytes 2147483648
    $dvd = Add-VMDvdDrive -VMName $vmName -Path $isoPath -Passthru
    Set-VMFirmware -VMName $vmName -FirstBootDevice $dvd
  } else {
    Set-VMFirmware -VMName $vmName -SecureBootTemplate MicrosoftUEFICertificateAuthority
  }
}

# (Re)branche une ISO sur une VM déjà existante (l'utilisateur a pu déposer
# une ISO depuis la dernière tentative).
function Attach-Iso([string]$isoPath) {
  Set-VMMemory -VMName $vmName -StartupBytes 2147483648
  Get-VMDvdDrive -VMName $vmName | Remove-VMDvdDrive -ErrorAction SilentlyContinue
  $dvd = Add-VMDvdDrive -VMName $vmName -Path $isoPath -Passthru
  Set-VMFirmware -VMName $vmName -FirstBootDevice $dvd
}

try {
  # Module Hyper-V présent ? (sinon : fonctionnalité Windows non activée)
  if (-not (Get-Command Get-VM -ErrorAction SilentlyContinue)) {
    Fail ("Hyper-V n'est pas activé sur ce Windows." + [char]10 + [char]10 + "Panneau de configuration > Programmes > Activer ou désactiver des fonctionnalités Windows > cocher Hyper-V, puis redémarre. (Windows Pro requis)" + [char]10 + [char]10 + "Astuce : le code secret lance aussi une VM sous Linux (QEMU/KVM).")
  }

  $iso = Find-Iso
  $isoPath = $null
  if ($iso) { $isoPath = $iso.FullName }

  $vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
  if (-not $vm) {
    New-HangarVm $isoPath
  } elseif ($isoPath) {
    Attach-Iso $isoPath
  }

  $vm = Get-VM -Name $vmName
  if ($vm.State -ne 'Running') {
    try {
      Start-VM -Name $vmName -ErrorAction Stop
    } catch {
      # AUTO-RÉPARATION : la VM existe mais refuse de démarrer — ex. bug
      # Hyper-V/Win11 « Microsoft Guest Runtime State ... .vmgs introuvable »
      # (fichier d'état jamais écrit pour une VM à moitié provisionnée).
      # Solution : vmms redémarré + VM supprimée + recréation propre + 2e essai.
      $firstErr = $_.Exception.Message
      Stop-VM -Name $vmName -TurnOff -Force -ErrorAction SilentlyContinue
      Restart-Service vmms -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 2
      Remove-VM -Name $vmName -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 1
      New-HangarVm $isoPath
      try {
        Start-VM -Name $vmName -ErrorAction Stop
      } catch {
        Fail ("HANGAR-OS a même réessayé après avoir recréé la VM, sans succès." + [char]10 + [char]10 + "Première erreur : " + $firstErr + [char]10 + [char]10 + "Deuxième erreur : " + $_.Exception.Message + [char]10 + [char]10 + "Solutions manuelles : 1) Gestionnaire Hyper-V > clic droit sur HANGAR33-VM > Supprimer, puis retape le code. 2) Redémarre le PC (le service Hyper-V repart propre).")
      }
    }
  }
  Start-Process vmconnect.exe -ArgumentList @('localhost', $vmName)
} catch {
  Fail ("HANGAR-OS n'a pas réussi à préparer la VM :" + [char]10 + $_.Exception.Message)
}
"#;

// ======================================================================
//  macOS : UTM (Virtualization.framework)
// ======================================================================

#[cfg(target_os = "macos")]
fn utm() -> Result<String, String> {
    Command::new("open")
        .args(["-a", "UTM"])
        .spawn()
        .map(|_| "UTM lancé — crée une VM et amuse-toi (Virtualization.framework).".into())
        .map_err(|_| "UTM introuvable : brew install --cask utm".into())
}

#[cfg(all(test, all(unix, not(target_os = "macos"))))]
mod tests {
    use super::*;

    #[test]
    fn la_chaine_accel_replie_vers_tcg() {
        let a = qemu_args();
        let i = a.iter().position(|x| x == "-accel").unwrap();
        assert_eq!(a[i + 1], "kvm");
        let j = a.iter().rposition(|x| x == "-accel").unwrap();
        assert_eq!(a[j + 1], "tcg");
        assert!(a.windows(2).any(|w| w[0] == "-display" && w[1] == "sdl"));
    }

    #[test]
    fn firmware_args_branche_code_et_vars() {
        let code = std::path::Path::new("/usr/share/OVMF/OVMF_CODE_4M.fd");
        let vars = std::path::Path::new("/tmp/vars.fd");
        let a = firmware_args(code, Some(vars));
        assert!(a.windows(2).any(|w| w[0] == "-drive"
            && w[1].contains("readonly=on")
            && w[1].contains("OVMF_CODE_4M")));
        assert!(a.windows(2).any(|w| w[0] == "-drive" && w[1].contains("/tmp/vars.fd")));
        // Sans vars : une seule drive.
        let b = firmware_args(code, None);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn les_isos_windows_sont_preferees() {
        let a = PathBuf::from("/dl/ubuntu-24.04.iso");
        let b = PathBuf::from("/dl/Win11_23H2_x64.iso");
        let c = PathBuf::from("/dl/fedora.iso");
        let pick = prefer_windows_iso(&[a.clone(), b.clone(), c.clone()]).unwrap();
        assert_eq!(pick, b);
        // Pas de Windows : première trouvée.
        let pick2 = prefer_windows_iso(&[a.clone(), c.clone()]).unwrap();
        assert_eq!(pick2, a);
        // Rien : None.
        assert!(prefer_windows_iso(&[]).is_none());
    }

    #[test]
    fn les_dossiers_iso_couvrent_les_usages() {
        let dirs = iso_search_dirs();
        assert!(dirs.iter().any(|d| d.to_string_lossy().contains("Téléchargements")
            || d.to_string_lossy().contains("Downloads")));
        assert!(dirs.iter().any(|d| d.to_string_lossy().contains("libvirt")));
    }
}
