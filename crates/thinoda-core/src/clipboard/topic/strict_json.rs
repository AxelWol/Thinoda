//! Strictly deserialize the versioned wire format without changing the model's
//! deliberately more permissive legacy document deserializers.

use serde::de::{
    self, Deserialize, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use std::collections::HashSet;
use std::fmt;

pub(super) fn from_str<'de, T: Deserialize<'de>>(json: &'de str) -> Result<T, String> {
    // Serde's BTreeMap deserializer replaces duplicate keys. Check all objects
    // first so duplicate task IDs (including aliases such as "1" and "01")
    // can never disappear before the model is validated.
    let mut scan = serde_json::Deserializer::from_str(json);
    UniqueJson::deserialize(&mut scan).map_err(|error| error.to_string())?;
    scan.end().map_err(|error| error.to_string())?;

    let mut decoder = serde_json::Deserializer::from_str(json);
    let value = T::deserialize(Strict(&mut decoder)).map_err(|error| error.to_string())?;
    decoder.end().map_err(|error| error.to_string())?;
    Ok(value)
}

struct UniqueJson;

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        decoder.deserialize_any(UniqueVisitor)
    }
}

#[derive(Hash, PartialEq, Eq)]
enum Key {
    Number(u64),
    Text(String),
}

struct UniqueVisitor;

impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = UniqueJson;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(UniqueJson)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        while sequence.next_element::<UniqueJson>()?.is_some() {}
        Ok(UniqueJson)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let identity = if !key.is_empty() && key.bytes().all(|byte| byte.is_ascii_digit()) {
                key.parse().map(Key::Number).unwrap_or(Key::Text(key))
            } else {
                Key::Text(key)
            };
            if !keys.insert(identity) {
                return Err(de::Error::custom("duplicate topic clipboard JSON key"));
            }
            map.next_value::<UniqueJson>()?;
        }
        Ok(UniqueJson)
    }
}

// Derived model deserializers normally ignore unknown fields to keep old
// documents loadable. On this explicitly versioned wire format, an unknown
// field could carry content a paste would silently discard. Propagate this
// adapter through every nested value and reject Serde's IgnoredAny requests.
struct Strict<D>(D);
struct StrictVisitor<V>(V);
struct StrictSeed<S>(S);
struct StrictSequence<A>(A);
struct StrictMap<A>(A);
struct StrictEnum<A>(A);
struct StrictVariant<A>(A);

macro_rules! delegate_deserialize {
    ($($method:ident),* $(,)?) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
                self.0.$method(StrictVisitor(visitor))
            }
        )*
    };
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Strict<D> {
    type Error = D::Error;

    delegate_deserialize!(
        deserialize_any,
        deserialize_bool,
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_i128,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_u128,
        deserialize_f32,
        deserialize_f64,
        deserialize_char,
        deserialize_str,
        deserialize_string,
        deserialize_bytes,
        deserialize_byte_buf,
        deserialize_option,
        deserialize_unit,
        deserialize_seq,
        deserialize_map,
        deserialize_identifier,
    );

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_unit_struct(name, StrictVisitor(visitor))
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0
            .deserialize_newtype_struct(name, StrictVisitor(visitor))
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_tuple(len, StrictVisitor(visitor))
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0
            .deserialize_tuple_struct(name, len, StrictVisitor(visitor))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0
            .deserialize_struct(name, fields, StrictVisitor(visitor))
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0
            .deserialize_enum(name, variants, StrictVisitor(visitor))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(de::Error::custom("unknown topic clipboard field"))
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

macro_rules! delegate_visit {
    ($($method:ident($value:ty)),* $(,)?) => {
        $(
            fn $method<E: de::Error>(self, value: $value) -> Result<Self::Value, E> {
                self.0.$method(value)
            }
        )*
    };
}

impl<'de, V: Visitor<'de>> Visitor<'de> for StrictVisitor<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.expecting(formatter)
    }

    delegate_visit!(
        visit_bool(bool),
        visit_i8(i8),
        visit_i16(i16),
        visit_i32(i32),
        visit_i64(i64),
        visit_i128(i128),
        visit_u8(u8),
        visit_u16(u16),
        visit_u32(u32),
        visit_u64(u64),
        visit_u128(u128),
        visit_f32(f32),
        visit_f64(f64),
        visit_char(char),
        visit_str(&str),
        visit_borrowed_str(&'de str),
        visit_string(String),
        visit_bytes(&[u8]),
        visit_borrowed_bytes(&'de [u8]),
        visit_byte_buf(Vec<u8>),
    );

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.visit_none()
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        self.0.visit_some(Strict(decoder))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        decoder: D,
    ) -> Result<Self::Value, D::Error> {
        self.0.visit_newtype_struct(Strict(decoder))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, sequence: A) -> Result<Self::Value, A::Error> {
        self.0.visit_seq(StrictSequence(sequence))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        self.0.visit_map(StrictMap(map))
    }

    fn visit_enum<A: EnumAccess<'de>>(self, value: A) -> Result<Self::Value, A::Error> {
        self.0.visit_enum(StrictEnum(value))
    }
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for StrictSeed<S> {
    type Value = S::Value;

    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(Strict(decoder))
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for StrictSequence<A> {
    type Error = A::Error;

    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, Self::Error> {
        self.0.next_element_seed(StrictSeed(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for StrictMap<A> {
    type Error = A::Error;

    fn next_key_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, Self::Error> {
        self.0.next_key_seed(StrictSeed(seed))
    }

    fn next_value_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<S::Value, Self::Error> {
        self.0.next_value_seed(StrictSeed(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for StrictEnum<A> {
    type Error = A::Error;
    type Variant = StrictVariant<A::Variant>;

    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Self::Variant), Self::Error> {
        self.0
            .variant_seed(StrictSeed(seed))
            .map(|(value, variant)| (value, StrictVariant(variant)))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for StrictVariant<A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        self.0.unit_variant()
    }

    fn newtype_variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<S::Value, Self::Error> {
        self.0.newtype_variant_seed(StrictSeed(seed))
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.tuple_variant(len, StrictVisitor(visitor))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.struct_variant(fields, StrictVisitor(visitor))
    }
}
