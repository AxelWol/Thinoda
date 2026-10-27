//! Bounded, sheet-owned embedded bytes. No file paths or URLs are dereferenced.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

pub const MAX_RESOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SHEET_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_WORKBOOK_RESOURCE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RESOURCES: usize = 128;
pub const RESOURCE_PREFIX: &str = "thinoda-resource:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    pub media_type: String,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    data: Arc<[u8]>,
}

impl Resource {
    pub fn new(name: &str, media_type: &str, data: Vec<u8>) -> Result<Self, String> {
        validate_metadata(name, media_type)?;
        if data.is_empty() || data.len() > MAX_RESOURCE_BYTES {
            return Err("embedded resource must contain 1 byte to 8 MiB".into());
        }
        Ok(Self {
            name: name.into(),
            media_type: media_type.into(),
            data: data.into(),
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn id(&self) -> String {
        format!("{:x}", Sha256::digest(&self.data))
    }
}

fn validate_metadata(name: &str, media_type: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 255
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        || matches!(name, "." | "..")
    {
        return Err("invalid embedded resource display name".into());
    }
    if media_type.is_empty()
        || media_type.len() > 127
        || !media_type.contains('/')
        || !media_type
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/.+-".contains(&c))
    {
        return Err("invalid embedded resource media type".into());
    }
    Ok(())
}

fn serialize_bytes<S: Serializer>(bytes: &Arc<[u8]>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&STANDARD.encode(bytes))
}

fn deserialize_bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<[u8]>, D::Error> {
    struct BytesVisitor;
    impl de::Visitor<'_> for BytesVisitor {
        type Value = Arc<[u8]>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("bounded base64 embedded bytes")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            if value.len() > MAX_RESOURCE_BYTES.div_ceil(3) * 4 {
                return Err(E::custom("embedded resource exceeds 8 MiB"));
            }
            let bytes = STANDARD.decode(value).map_err(E::custom)?;
            if bytes.is_empty() || bytes.len() > MAX_RESOURCE_BYTES {
                return Err(E::custom("embedded resource must contain 1 byte to 8 MiB"));
            }
            Ok(bytes.into())
        }
    }
    deserializer.deserialize_str(BytesVisitor)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceStore {
    pub version: u32,
    #[serde(deserialize_with = "deserialize_entries")]
    pub entries: BTreeMap<String, Resource>,
}

fn deserialize_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Resource>, D::Error> {
    struct EntriesVisitor;
    impl<'de> de::Visitor<'de> for EntriesVisitor {
        type Value = BTreeMap<String, Resource>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most 128 resources totaling at most 32 MiB")
        }

        fn visit_map<A: de::MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            let mut total = 0usize;
            while let Some(id) = access.next_key::<String>()? {
                if entries.len() >= MAX_RESOURCES || entries.contains_key(&id) || id.len() != 64 {
                    return Err(de::Error::custom(
                        "invalid, duplicate or excessive embedded resource IDs",
                    ));
                }
                let resource: Resource = access.next_value()?;
                total = total.saturating_add(resource.data.len());
                if total > MAX_SHEET_RESOURCE_BYTES {
                    return Err(de::Error::custom("sheet embedded resources exceed 32 MiB"));
                }
                entries.insert(id, resource);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(EntriesVisitor)
}

impl Default for ResourceStore {
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl ResourceStore {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn byte_len(&self) -> usize {
        self.entries.values().fold(0usize, |total, resource| {
            total.saturating_add(resource.data.len())
        })
    }

    /// Equal bytes share an ID; the existing display metadata is kept on deduplication.
    pub fn insert(&mut self, resource: Resource) -> Result<String, String> {
        self.validate()?;
        validate_metadata(&resource.name, &resource.media_type)?;
        let id = resource.id();
        if !self.entries.contains_key(&id) {
            if self.entries.len() >= MAX_RESOURCES
                || self.byte_len().saturating_add(resource.data.len()) > MAX_SHEET_RESOURCE_BYTES
            {
                return Err("sheet embedded resources exceed 128 entries or 32 MiB".into());
            }

            self.entries.insert(id.clone(), resource);
        }
        Ok(format!("{RESOURCE_PREFIX}{id}"))
    }

    /// Validate the complete union before callers mutate topics or their resource store.
    pub fn merged(&self, incoming: impl IntoIterator<Item = Resource>) -> Result<Self, String> {
        self.validate()?;
        let mut result = self.clone();
        for resource in incoming {
            result.insert(resource)?;
        }
        Ok(result)
    }

    pub fn get(&self, reference: &str) -> Result<&Resource, String> {
        let id = reference
            .strip_prefix(RESOURCE_PREFIX)
            .ok_or("not an embedded resource reference")?;
        self.entries
            .get(id)
            .ok_or_else(|| format!("missing embedded resource {id}"))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported embedded resource version {}",
                self.version
            ));
        }
        if self.entries.len() > MAX_RESOURCES || self.byte_len() > MAX_SHEET_RESOURCE_BYTES {
            return Err("sheet embedded resources exceed 128 entries or 32 MiB".into());
        }
        for (id, resource) in &self.entries {
            validate_metadata(&resource.name, &resource.media_type)?;
            if resource.data.is_empty() || resource.data.len() > MAX_RESOURCE_BYTES {
                return Err("embedded resource must contain 1 byte to 8 MiB".into());
            }
            if *id != resource.id() {
                return Err(format!("embedded resource digest mismatch: {id}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_ids_deduplicate_and_json_preserves_bytes() {
        let mut store = ResourceStore::default();
        let reference = store
            .insert(Resource::new("Evidence.txt", "text/plain", b"evidence".to_vec()).unwrap())
            .unwrap();
        assert_eq!(
            store
                .insert(Resource::new("Other.txt", "text/plain", b"evidence".to_vec()).unwrap())
                .unwrap(),
            reference
        );
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.get(&reference).unwrap().name, "Evidence.txt");
        let restored: ResourceStore =
            serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        assert_eq!(restored, store);
        restored.validate().unwrap();
        store.entries.values_mut().next().unwrap().data = Arc::from(&b"corrupt"[..]);
        assert!(store.validate().unwrap_err().contains("digest mismatch"));
    }

    #[test]
    fn invalid_names_versions_missing_bytes_and_limits_are_errors() {
        for name in ["", "..", "../file", "C:\\file", "a/b", "line\nbreak"] {
            assert!(Resource::new(name, "text/plain", vec![1]).is_err());
        }
        assert!(Resource::new("valid", "text/plain", vec![]).is_err());
        assert!(Resource::new("valid", "text/plain", vec![0; MAX_RESOURCE_BYTES + 1]).is_err());
        assert!(Resource::new("valid", "text/plain; x=1", vec![1]).is_err());
        let mut store = ResourceStore::default();
        assert!(store.get("thinoda-resource:missing").is_err());
        for index in 0..MAX_RESOURCES {
            store
                .insert(
                    Resource::new(
                        "file",
                        "application/octet-stream",
                        index.to_le_bytes().to_vec(),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let before = store.clone();
        assert!(
            store
                .insert(Resource::new("overflow", "text/plain", vec![255; 20]).unwrap())
                .is_err()
        );
        assert_eq!(store, before);
        store.version = 2;
        assert!(store.validate().unwrap_err().contains("unsupported"));
    }
}
