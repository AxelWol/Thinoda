use thinoda_io::drawio_bundle::{fuzzing_catalog, fuzzing_decode};

pub fn exercise(data: &[u8]) {
    if data.len() > 1_048_576 {
        return;
    }
    let separator = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    let catalog = &data[..separator];
    let blob = data.get(separator + 1..).unwrap_or_default();
    let Some(bundle) = fuzzing_catalog(catalog) else {
        return;
    };
    for (index, entry) in bundle.catalog().packages.iter().take(4).enumerate() {
        if let Some(shape) = entry.shapes.first() {
            let alias = entry.alias(shape);
            assert!(bundle.has_alias(&alias));
            let _ = bundle.lookup(&alias);
        }
        let _ = bundle.package(index);
        let _ = bundle.package_by_id(&entry.id);
        if let Some(package) = fuzzing_decode(entry, blob) {
            assert_eq!(package.id, entry.id);
        }
    }
}
