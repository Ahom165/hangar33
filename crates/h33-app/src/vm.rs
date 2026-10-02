//! Easter egg HANGAR-OS : le code secret du terminal lance une VRAIE
//! machine virtuelle sur la machine hôte — parce que c'est drôle.
//!
//! Selon l'OS :
//!  - Linux/BSD : QEMU avec KVM (`-accel kvm`), repli automatique TCG si
//!    /dev/kvm est absent (chaîne `-accel kvm -accel tcg`).
//!  - Windows   : Hyper-V via un script PowerShell autonome (écrit dans
//!    %TEMP%) qui se réélève en UAC si besoin, CRÉE "HANGAR33-VM" si elle
//!    n'existe pas, la démarre et ouvre la console VMConnect. Sans droits
//!    ou sans Hyper-V : message clair (et pas l'erreur cryptique de
//!    vmconnect qui cherchait une VM inexistante sur localhost).
//!  - macOS     : UTM (front-end Virtualization.framework) s'il est là.
//!
//! Aucune sortie n'est attendue du process : on spawn détaché et on
//! rend la main immédiatement (le jeu ne se fige jamais sur une blague).

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
//  Linux / BSD : QEMU + KVM (repli TCG)
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

#[cfg(all(unix, not(target_os = "macos")))]
fn qemu_kvm() -> Result<String, String> {
    // QEMU présent ? (sinon on tente quand même : message d'erreur propre)
    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.args(qemu_args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(_child) => {
            let kvm = std::path::Path::new("/dev/kvm").exists();
            Ok(if kvm {
                "VM lancée : QEMU/KVM (accélération matérielle). Regarde ton écran : SeaBIOS boot !".into()
            } else {
                "VM lancée : QEMU/TCG (pas de /dev/kvm, émulation logicielle). C'est une VRAIE VM, juste lente.".into()
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(
            "QEMU introuvable. Sur Debian/Ubuntu : sudo apt install qemu-system-x86".into(),
        ),
        Err(e) => Err(format!("Impossible de lancer QEMU : {e}")),
    }
}

// ======================================================================
//  Windows : Hyper-V — script autonome + élévation UAC + création VM
// ======================================================================

#[cfg(target_os = "windows")]
fn hyper_v() -> Result<String, String> {
    // Le script PowerShell (HYPERV_PS1) fait TOUT le travail dans un
    // process détaché : vérif des droits, ré-élévation UAC ("sudo" du
    // terminal), création de la VM si absente, démarrage, VMConnect.
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
             la VM '{VM_NAME}' est créée si besoin, démarre, et sa console s'ouvre."
        ))
        .map_err(|e| format!("Impossible de lancer PowerShell : {e}"))
}

/// Script d'installation/lancement de la VM Hyper-V (voir `hyper_v`).
/// tout en une passe : élévation, création, démarrage, console.
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

try {
  # Module Hyper-V présent ? (sinon : fonctionnalité Windows non activée)
  if (-not (Get-Command Get-VM -ErrorAction SilentlyContinue)) {
    Fail ("Hyper-V n'est pas activé sur ce Windows." + [char]10 + [char]10 + "Panneau de configuration > Programmes > Activer ou désactiver des fonctionnalités Windows > cocher Hyper-V, puis redémarre. (Windows Pro requis)" + [char]10 + [char]10 + "Astuce : le code secret lance aussi une VM sous Linux (QEMU/KVM).")
  }
  $vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
  if (-not $vm) {
    # Gen 2, 512 Mo, sans disque : la VM boote en PXE — c'est déjà très drôle.
    $vm = New-VM -Name $vmName -Generation 2 -MemoryStartupBytes 536870912 -NoVHD
    Set-VMFirmware -VMName $vmName -SecureBootTemplate MicrosoftUEFICertificateAuthority
  }
  if ($vm.State -ne 'Running') { Start-VM -Name $vmName }
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
}
