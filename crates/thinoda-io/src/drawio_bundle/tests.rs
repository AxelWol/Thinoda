use super::*;
use std::collections::HashSet;

const FUZZ_SEED: &[u8] = include_bytes!("../../../../fuzz/corpus/drawio_bundle/valid-package");

fn fuzz_seed() -> (Bundle, &'static [u8]) {
    let separator = FUZZ_SEED.iter().position(|byte| *byte == 0).unwrap();
    let bundle = fuzzing_catalog(&FUZZ_SEED[..separator]).expect("seed catalog");
    (bundle, &FUZZ_SEED[separator + 1..])
}

#[test]
fn catalog_is_consistent_with_the_package_blob() {
    let bundle = Bundle::new();
    let catalog = bundle.catalog();
    assert_eq!(catalog.format, BUNDLE_FORMAT);
    assert_eq!(catalog.license, "Apache-2.0");
    assert_eq!(
        catalog.translator_revision,
        crate::drawio_stencil::TRANSLATOR_REVISION,
        "the translator changed: regenerate the bundle"
    );
    let families: HashSet<&str> = catalog
        .packages
        .iter()
        .map(|package| package.family.as_str())
        .collect();
    assert_eq!(families, FAMILIES.into_iter().collect());
    let mut offset = 0;
    let mut ids = HashSet::new();
    let mut aliases = HashSet::new();
    for package in &catalog.packages {
        assert!(ids.insert(&package.id), "duplicate package {}", package.id);
        assert_eq!(package.offset, offset, "{}", package.id);
        offset += package.length;
        assert_eq!(package.sha256.len(), 64);
        assert_eq!(package.namespace, package.namespace.to_lowercase());
        assert!(package.namespace.starts_with("mxgraph."));
        assert!(!package.shapes.is_empty(), "{}", package.id);
        for shape in &package.shapes {
            let alias = package.alias(shape);
            assert!(!alias.contains(' '), "{alias}");
            assert_eq!(alias, alias.to_lowercase());
            assert!(aliases.insert(alias.clone()), "duplicate alias {alias}");
        }
    }
    assert_eq!(offset, PACKAGES.len());
    assert_eq!(bundle.decoded(), 0, "reading the catalog decodes nothing");
}

#[test]
fn every_bundled_package_decodes_and_draws_its_catalog_shapes() {
    let bundle = Bundle::new();
    for (index, entry) in bundle.catalog().packages.iter().enumerate() {
        let package = bundle.package(index).expect(&entry.id);
        assert_eq!(package.license.as_deref(), Some("Apache-2.0"));
        assert_eq!(package.revision, bundle.catalog().revision);
        assert!(!package.extensions.contains_key("drawio.import"));
        for shape in &entry.shapes {
            let definition = package
                .definition(&shape.definition)
                .unwrap_or_else(|| panic!("{} lacks {}", entry.id, shape.definition));
            assert!(definition.requires.is_empty(), "{}", shape.definition);
            assert_eq!(definition.title, shape.title);
            let style = definition.interchange["drawio"]["style"].as_str();
            assert_eq!(
                style,
                Some(format!("shape={};", entry.alias(shape)).as_str())
            );
            let stencil = thinoda_core::stencil_object::StencilRef::with_package(
                Arc::clone(&package),
                shape.definition.clone(),
            );
            assert!(stencil.resolve().is_some(), "{}", entry.alias(shape));
        }
    }
    assert_eq!(bundle.decoded(), bundle.catalog().packages.len());
}

#[test]
fn lookups_decode_only_the_package_they_need() {
    let bundle = Bundle::new();
    assert!(bundle.has_alias("mxgraph.electrical.logic_gates.and"));
    assert!(bundle.has_alias("mxgraph.flowchart.annotation_1"));
    assert!(!bundle.has_alias("mxgraph.aws4.lambda"));
    assert_eq!(bundle.decoded(), 0);
    let (package, definition) = bundle.lookup("mxgraph.electrical.logic_gates.and").unwrap();
    assert_eq!(package.id, "drawio.electrical.logic_gates");
    assert!(package.definition(definition).is_some());
    assert_eq!(bundle.decoded(), 1);
    let again = bundle
        .package_by_id("drawio.electrical.logic_gates")
        .unwrap();
    assert!(Arc::ptr_eq(&package, &again), "decoded packages are cached");
    assert_eq!(bundle.decoded(), 1);
}

#[test]
fn bundle_stays_within_its_release_size_budget() {
    assert!(
        CATALOG.len() + PACKAGES.len() <= 1_000_000,
        "bundled draw.io stencils add {} bytes",
        CATALOG.len() + PACKAGES.len()
    );
    for package in &Bundle::new().catalog().packages {
        assert!(package.length > 0, "{}", package.id);
        assert!(
            package.inflated <= Limits::DEFAULT.max_package_bytes,
            "{} exceeds the stencil inflation limit",
            package.id
        );
    }
}

#[test]
fn fuzz_seed_reaches_catalog_indexing_and_package_decoding() {
    let (bundle, blob) = fuzz_seed();
    assert_eq!(bundle.catalog().packages.len(), 1);
    let entry = &bundle.catalog().packages[0];
    let shipped = Bundle::new();
    let original = shipped
        .catalog()
        .packages
        .iter()
        .find(|package| package.id == entry.id)
        .expect("seed package is still bundled");
    assert_eq!(entry.sha256, original.sha256);
    assert_eq!(entry.inflated, original.inflated);
    assert_eq!(entry.length, original.length);
    assert_eq!(
        blob,
        &PACKAGES[original.offset..original.offset + original.length]
    );
    let alias = entry.alias(&entry.shapes[0]);
    assert!(bundle.has_alias(&alias));
    let decoded = fuzzing_decode(entry, blob).expect("seed package");
    assert_eq!(decoded.id, entry.id);
    assert!(decoded.definition(&entry.shapes[0].definition).is_some());

    let mut catalog: serde_json::Value =
        serde_json::from_slice(&FUZZ_SEED[..FUZZ_SEED.len() - blob.len() - 1]).unwrap();
    catalog["format"] = serde_json::json!(BUNDLE_FORMAT + 1);
    assert!(fuzzing_catalog(&serde_json::to_vec(&catalog).unwrap()).is_none());
    assert!(fuzzing_catalog(b"{invalid").is_none());
}

#[test]
fn catalog_index_rejects_amplified_alias_keys() {
    let separator = FUZZ_SEED.iter().position(|byte| *byte == 0).unwrap();
    let mut catalog: serde_json::Value = serde_json::from_slice(&FUZZ_SEED[..separator]).unwrap();
    let entry = &mut catalog["packages"][0];
    let namespace = "n".repeat(262_144);
    entry["namespace"] = serde_json::json!(namespace);
    let prototype = entry["shapes"][0].clone();
    let shapes: Vec<_> = (0..17)
        .map(|index| {
            let mut shape = prototype.clone();
            shape[0] = serde_json::json!(format!("shape{index}"));
            shape
        })
        .collect();
    entry["shapes"] = serde_json::json!(&shapes[..15]);
    let accepted = serde_json::to_vec(&catalog).unwrap();
    let bundle = fuzzing_catalog(&accepted).expect("index within the budget");
    assert!(bundle.has_alias(&format!("{namespace}.shape14")));
    catalog["packages"][0]["shapes"] = serde_json::json!(shapes);
    let bytes = serde_json::to_vec(&catalog).unwrap();
    assert!(bytes.len() < 1_048_576);
    assert!(fuzzing_catalog(&bytes).is_none());
}

#[test]
fn decoder_rejects_invalid_ranges_lengths_and_packages() {
    let (bundle, blob) = fuzz_seed();
    let valid = &bundle.catalog().packages[0];

    let mut entry = valid.clone();
    entry.offset = usize::MAX;
    entry.length = 1;
    assert!(fuzzing_decode(&entry, blob).is_none());

    entry.offset = blob.len();
    assert!(fuzzing_decode(&entry, blob).is_none());

    entry = valid.clone();
    entry.length -= 1;
    assert!(fuzzing_decode(&entry, blob).is_none());

    entry = valid.clone();
    entry.inflated += 1;
    assert!(fuzzing_decode(&entry, blob).is_none());

    entry = valid.clone();
    entry.id.push_str(".different");
    assert!(fuzzing_decode(&entry, blob).is_none());

    for bytes in [&b"\xff"[..], &b"not JSON"[..]] {
        let compressed = miniz_oxide::deflate::compress_to_vec(bytes, 6);
        entry = valid.clone();
        entry.length = compressed.len();
        entry.inflated = bytes.len();
        assert!(fuzzing_decode(&entry, &compressed).is_none());
    }

    let oversized = vec![b' '; Limits::DEFAULT.max_package_bytes + 1];
    let compressed = miniz_oxide::deflate::compress_to_vec(&oversized, 6);
    entry = valid.clone();
    entry.length = compressed.len();
    entry.inflated = oversized.len();
    assert!(fuzzing_decode(&entry, &compressed).is_none());
}
