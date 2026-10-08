//! Opt-in, deterministic offline probe diagnostics. No runtime consumer.

use std::io::{BufWriter, Write as _};

use serde::Serialize;

use super::{Accumulator, ProbeField, TransportReceiver, TransportScene, compress};

/// Opt-in dump directory; use a separate directory for each source/build.
pub const DUMP_ENV: &str = "PLACES_PROBE_DUMP_DIR";

#[derive(Clone, Debug, Serialize)]
pub(super) struct ProbeAudit {
    pub position: [f32; 3],
    pub target_room: i32,
    pub direct: [f32; 3],
    pub direct_moment: [f32; 3],
    pub indirect: [f32; 3],
    pub indirect_moment: [f32; 3],
    pub surface_hits: usize,
    pub escaping_rays: usize,
    pub zero_radiance_hits: usize,
    pub visible_emitters: Vec<usize>,
}

impl ProbeAudit {
    pub(super) fn new(position: [f32; 3], target_room: i32, direct: &Accumulator) -> Self {
        let compressed_direct = compress(direct);
        Self {
            position,
            target_room,
            direct: compressed_direct.irradiance,
            direct_moment: compressed_direct.direction,
            indirect: [0.0; 3],
            indirect_moment: [0.0; 3],
            surface_hits: 0,
            escaping_rays: 0,
            zero_radiance_hits: 0,
            visible_emitters: Vec::new(),
        }
    }
}

/// Writes the unfiltered physical surface receivers as a comparison oracle.
pub(super) fn dump_receivers(
    scene: &TransportScene,
    receivers: &[TransportReceiver],
    values: &[Accumulator],
    bounces: u8,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct Surface {
        position: [f32; 3],
        normal: [f32; 3],
        triangle: u32,
        physical: [f32; 3],
        filled: [f32; 3],
    }
    if super::diagnostics::directory(DUMP_ENV).is_none() {
        return Ok(());
    }
    let stride = receivers.len().div_ceil(65_536).max(1);
    let surfaces: Vec<_> = receivers
        .iter()
        .zip(values)
        .enumerate()
        .step_by(stride)
        .map(|(index, (receiver, value))| {
            let mut texel = super::compress_surface(value, receiver.normal);
            if let Some(target) = scene.receiver_target.get(index) {
                super::fill_texel(
                    &mut texel,
                    receiver.normal,
                    super::scale(
                        super::attenuate(*target, receiver.attenuation),
                        scene.baseline_support(receiver.position),
                    ),
                );
            }
            Surface {
                position: receiver.position,
                normal: receiver.normal,
                triangle: receiver.surface,
                physical: value.surface_light,
                filled: texel.light_at(receiver.normal),
            }
        })
        .collect();
    write_dump(&format!("surfaces-b{bounces}.json"), &surfaces)
}

pub(super) fn dump_bake(
    field: &ProbeField,
    audits: &[Option<ProbeAudit>],
    rays: usize,
    bounces: u8,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct Bake<'a> {
        origin: [f32; 3],
        cell_m: f32,
        dims: [u32; 3],
        gather_rays: usize,
        probes: &'a [Option<ProbeAudit>],
        combined: Vec<[f32; 3]>,
        moments: Vec<[f32; 3]>,
    }
    if super::diagnostics::directory(DUMP_ENV).is_none() {
        return Ok(());
    }
    write_dump(
        &format!("probes-b{bounces}.json"),
        &Bake {
            origin: field.min,
            cell_m: field.cell_m,
            dims: field.dims,
            gather_rays: rays,
            probes: audits,
            combined: field.probes.iter().map(|probe| probe.irradiance).collect(),
            moments: field.probes.iter().map(|probe| probe.direction).collect(),
        },
    )
}

/// Records final compiler validity labels alongside the baker's raw dump.
///
/// # Errors
/// Returns a named error when the requested diagnostic cannot be written.
pub fn dump_labels(field: &ProbeField, quality: &str) -> Result<(), String> {
    if super::diagnostics::directory(DUMP_ENV).is_none() {
        return Ok(());
    }
    write_dump(
        &format!("labels-{quality}.json"),
        &field
            .probes
            .iter()
            .map(|probe| probe.room)
            .collect::<Vec<_>>(),
    )
}

fn write_dump(name: &str, value: &impl Serialize) -> Result<(), String> {
    let Some(dump_path) = super::diagnostics::directory(DUMP_ENV) else {
        return Ok(());
    };
    std::fs::create_dir_all(&dump_path)
        .map_err(|error| format!("probe dump directory: {error}"))?;
    let path = dump_path.join(name);
    if path.exists() {
        return Err(format!("probe dump refuses existing {}", path.display()));
    }
    // A failed serialization must not publish a partial diagnostic. Creation
    // is exclusive so a pre-existing temporary file/symlink is never followed.
    let temporary = dump_path.join(format!(".{name}.{}.tmp", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("probe dump {}: {error}", temporary.display()))?;
    let result = (|| {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, value)
            .map_err(|error| format!("probe dump serialization: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("probe dump flush: {error}"))?;
        std::fs::hard_link(&temporary, &path)
            .map_err(|error| format!("probe dump publish: {error}"))?;
        std::fs::remove_file(&temporary).map_err(|error| format!("probe dump cleanup: {error}"))
    })();
    if let Err(error) = result {
        if let Err(cleanup_error) = std::fs::remove_file(&temporary)
            && cleanup_error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!(
                "{error}; probe dump cleanup {}: {cleanup_error}",
                temporary.display()
            ));
        }
        return Err(error);
    }
    Ok(())
}
