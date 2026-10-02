//! Assets GPU embarqués : les GLB exportés depuis Blender (pipeline MCP)
//! sont compilés dans le binaire (`include_bytes!`) — pas d'IO runtime,
//! pas d'asset manquant possible.

use crate::gltf_asset::load_glb_or_panic;
use crate::mesh::Mesh;

pub struct GpuAssets {
    /// Enveloppe du hangar (sol, murs, charpente, toiture, lampes, props).
    pub shell: Mesh,
    /// Machines, indexées par `MachineKind as usize` (ordre h33-core).
    pub machines: [Mesh; 8],
    /// Colis fermé (carton kraft + adhésif).
    pub package: Mesh,
    /// Ordinateur de gestion (HANGAR-OS) posé dans le hangar.
    pub computer: Mesh,
}

impl GpuAssets {
    pub fn load(device: &wgpu::Device) -> Self {
        const SHELL: &[u8] = include_bytes!("../../../assets/models/hangar_shell.glb");
        const M_UNBOXER: &[u8] = include_bytes!("../../../assets/models/machine_unboxer.glb");
        const M_SELLER: &[u8] = include_bytes!("../../../assets/models/machine_seller.glb");
        const M_INCINERATOR: &[u8] = include_bytes!("../../../assets/models/machine_incinerator.glb");
        const M_SPLITTER: &[u8] = include_bytes!("../../../assets/models/machine_splitter.glb");
        const M_SCANNER: &[u8] = include_bytes!("../../../assets/models/machine_scanner.glb");
        const M_AUTOBUYER: &[u8] = include_bytes!("../../../assets/models/machine_autobuyer.glb");
        const M_ROBOT_ARM: &[u8] = include_bytes!("../../../assets/models/machine_robotarm.glb");
        const M_GENERATOR: &[u8] = include_bytes!("../../../assets/models/machine_generator.glb");
        const PACKAGE: &[u8] = include_bytes!("../../../assets/models/package.glb");
        const COMPUTER: &[u8] = include_bytes!("../../../assets/models/computer.glb");

        let load = |bytes: &[u8], name: &str| {
            let m = load_glb_or_panic(bytes, name);
            println!(
                "[h33] GLB {name}: {} sommets, {} triangles",
                m.vertices.len(),
                m.indices.len() / 3
            );
            Mesh::from_raw(device, name, &m.vertices, &m.normals, &m.colors, &m.indices)
        };

        Self {
            shell: load(SHELL, "hangar_shell"),
            machines: [
                load(M_UNBOXER, "machine_unboxer"),          // Unpacker
                load(M_SELLER, "machine_seller"),            // Seller
                load(M_INCINERATOR, "machine_incinerator"),  // Incinerator
                load(M_SPLITTER, "machine_splitter"),        // Splitter
                load(M_SCANNER, "machine_scanner"),          // RingScanner
                load(M_AUTOBUYER, "machine_autobuyer"),      // AutoBuyer
                load(M_ROBOT_ARM, "machine_robotarm"),       // RobotArm
                load(M_GENERATOR, "machine_generator"),      // Generator
            ],
            package: load(PACKAGE, "package"),
            computer: load(COMPUTER, "computer"),
        }
    }
}
