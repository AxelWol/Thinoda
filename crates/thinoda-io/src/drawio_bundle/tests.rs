use super::*;
use std::collections::HashSet;

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
}
