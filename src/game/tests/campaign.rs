//! One tiny save guards the actual disk round trip, bounded admission and
//! idempotence across app restarts. No environment mutation, asset load or bake.
use super::{CampaignProgress, read, write};

#[test]
fn completion_save_roundtrips_atomically_and_refuses_bad_or_oversized_data() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-save-{}", std::process::id()));
    let path = folder.join("progress.json");
    let mut progress = CampaignProgress::default();
    for id in ["beach", "home", "arcade", "beach"] {
        progress.complete(id)?;
    }
    write(&path, &progress)?;
    let restored = read(&path)?;
    assert_eq!(
        restored.count(),
        3,
        "repeat completions cannot duplicate a reward"
    );
    assert!(
        restored.completed("home"),
        "completion survives a new reader"
    );
    assert!(
        !path.with_extension("json.tmp").exists(),
        "publication consumes the temporary file"
    );
    progress.complete("winter")?;
    write(&path, &progress)?;
    assert_eq!(
        read(&path)?.count(),
        4,
        "an existing save is atomically replaced"
    );
    std::fs::write(&path, b"{\"completed\":[\"bad\\nidentity\"]}")
        .map_err(|problem| problem.to_string())?;
    assert!(
        read(&path).is_err(),
        "malformed saved identities are refused"
    );
    std::fs::write(&path, vec![b' '; 16_385]).map_err(|problem| problem.to_string())?;
    assert!(
        read(&path).is_err(),
        "save reads stop after the small admission bound"
    );
    std::fs::remove_file(&path).map_err(|problem| problem.to_string())?;
    std::fs::remove_dir(&folder).map_err(|problem| problem.to_string())?;
    Ok(())
}
