//! AIR-80 — `tour materialize` surface: manifest provenance roundtrip
//! (`[[delta_arc]]`) and snapshot-pair resolution semantics.

use code_reality::tour::snapshot_for;
use code_reality::tour_manifest::{dump, load, upsert_delta_arc};

#[test]
fn delta_arc_roundtrip_and_replace_by_arcid() {
    let tmp = std::env::temp_dir().join(format!(
        "cr-s8-manifest-{}",
        std::process::id() as u64
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("manifest.toml");

    let mut m = load(&path).unwrap(); // absent → default
    upsert_delta_arc(
        &mut m,
        &[
            ("arcId", toml::Value::String("air-78".into())),
            ("cardId", toml::Value::String("AIR-78".into())),
            ("base", toml::Value::String("beb8642".into())),
            ("target", toml::Value::String("0d97bd7".into())),
            (
                "ep",
                toml::Value::String("ai-analysis/_tasks/x/ep.md".into()),
            ),
            ("quality", toml::Value::String("full".into())),
        ],
    );
    upsert_delta_arc(
        &mut m,
        &[
            ("arcId", toml::Value::String("air-79".into())),
            ("base", toml::Value::String("a".into())),
            ("target", toml::Value::String("b".into())),
        ],
    );
    // same arcId re-materialize → authoritative full replace (canonical key)
    upsert_delta_arc(
        &mut m,
        &[
            ("arcId", toml::Value::String("air-78".into())),
            ("base", toml::Value::String("beb8642".into())),
            ("target", toml::Value::String("0d97bd7".into())),
            ("quality", toml::Value::String("degraded".into())),
            (
                "tourPath",
                toml::Value::String(".tours/delta/air-78.tour".into()),
            ),
        ],
    );
    dump(&path, &m).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[[delta_arc]]"), "{text}");

    let back = load(&path).unwrap();
    assert_eq!(back.delta_arc.len(), 2);
    let row78 = back
        .delta_arc
        .iter()
        .find(|r| r.get("arcId").and_then(|v| v.as_str()) == Some("air-78"))
        .unwrap();
    // full replace: cardId/ep from the first write are gone (tool-authoritative)
    assert!(row78.get("cardId").is_none());
    assert!(row78.get("ep").is_none());
    assert_eq!(
        row78.get("quality").and_then(|v| v.as_str()),
        Some("degraded")
    );
    assert_eq!(
        row78.get("tourPath").and_then(|v| v.as_str()),
        Some(".tours/delta/air-78.tour")
    );
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn snapshot_resolution_hit_miss_ambiguous() {
    let tmp = std::env::temp_dir().join(format!(
        "cr-s8-snap-{}",
        std::process::id() as u64
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("ai-rules-beb86429.json"), "{}").unwrap();

    // hit: sha8 suffix match (commit longer than 8)
    let hit = snapshot_for(&tmp, "beb864291234567890abcdef").unwrap();
    assert!(hit.ends_with("ai-rules-beb86429.json"));

    // miss: fail-loud with the expected pattern + ask-once hint
    let err = snapshot_for(&tmp, "fffffffffffffff").unwrap_err();
    assert!(err.contains("-ffffffff.json"), "{err}");
    assert!(err.contains("snapshot 缺席"), "{err}");

    // ambiguous: two files share the sha8 suffix
    std::fs::write(tmp.join("other-beb86429.json"), "{}").unwrap();
    let amb = snapshot_for(&tmp, "beb86429").unwrap_err();
    assert!(amb.contains("歧義"), "{amb}");
    std::fs::remove_dir_all(&tmp).ok();
}
