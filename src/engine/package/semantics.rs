//! Runtime semantic admission before typed or editable JSON collection growth.
use crate::engine::{level::LevelDef, resources};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::{cell::RefCell, fmt, rc::Rc};
type Charge = Rc<RefCell<resources::Reservation>>;

pub fn parse(text: &str) -> Result<LevelDef, String> {
    let _scope = resources::admission_scope();
    resources::checkpoint()?;
    // Covers owned strings, escaped-string scratch and serde enum buffers while
    // the typed parse is active. Collection storage is charged separately below.
    let strings =
        resources::allocation(text.len().checked_mul(4).ok_or("Semantic size overflow")?)?;
    let mut counts = serde_json::Deserializer::from_str(text);
    let _counts = Counts::deserialize(&mut counts).map_err(|error| error.to_string())?;
    counts.end().map_err(|error| error.to_string())?;
    drop(counts);
    drop(strings);
    decode(text)
}

pub fn decode<'de, T: Deserialize<'de>>(text: &'de str) -> Result<T, String> {
    let _scope = resources::admission_scope();
    let _strings =
        resources::allocation(text.len().checked_mul(4).ok_or("Semantic size overflow")?)?;
    let _root = resources::allocation(
        std::mem::size_of::<T>()
            .checked_mul(8)
            .ok_or("Semantic root size overflow")?,
    )?;
    let mut decoder = serde_json::Deserializer::from_str(text);
    let value = T::deserialize(Bounded(
        &mut decoder,
        Rc::new(RefCell::new(resources::allocation(0)?)),
    ))
    .map_err(|error| error.to_string())?;
    decoder.end().map_err(|error| error.to_string())?;
    Ok(value)
}

// These existing hard limits must refuse before constructing any typed elements.
// Unknown and nested fields are still allocation-bounded by the typed adapter.
#[derive(Deserialize)]
struct Counts {
    #[serde(default, rename = "rooms", deserialize_with = "rooms")]
    _rooms: u64,
    #[serde(default, rename = "walls", deserialize_with = "walls")]
    _walls: u64,
    #[serde(default, rename = "props", deserialize_with = "props")]
    _props: u64,
    #[serde(
        default,
        rename = "ceiling_lights",
        deserialize_with = "ceiling_lights"
    )]
    _ceiling_lights: u64,
    #[serde(default, rename = "decals", deserialize_with = "decals")]
    _decals: u64,
    #[serde(default, rename = "floor_patches", deserialize_with = "floor_patches")]
    _floor_patches: u64,
}
macro_rules! count_field {
    ($name:ident,$limit:ident) => {
        fn $name<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
            struct Count;
            impl<'de> Visitor<'de> for Count {
                type Value = u64;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a semantic collection")
                }
                fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<u64, A::Error> {
                    let mut n = 0_u64;
                    while a.next_element::<de::IgnoredAny>()?.is_some() {
                        resources::checkpoint().map_err(de::Error::custom)?;
                        n = n.saturating_add(1);
                        if n > crate::engine::level::$limit {
                            return Err(de::Error::custom(concat!(
                                "Level contains too many ",
                                stringify!($name)
                            )));
                        }
                    }
                    Ok(n)
                }
            }
            d.deserialize_seq(Count)
        }
    };
}
count_field!(rooms, MAX_LEVEL_ROOMS);
count_field!(walls, MAX_LEVEL_WALLS);
count_field!(props, MAX_LEVEL_PROPS);
count_field!(ceiling_lights, MAX_LEVEL_CEILING_LIGHTS);
count_field!(decals, MAX_LEVEL_DECALS);
count_field!(floor_patches, MAX_LEVEL_FLOOR_PATCHES);

struct Bounded<D>(D, Charge);
struct Checked<V>(V, Charge);
struct Seed<S>(S, Charge);
impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<S> {
    type Value = S::Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(Bounded(d, self.1))
    }
}
// Suppress untrusted size hints; charge before a seed can allocate or its caller
// can grow a Vec/map. Eight slots cover Vec minimum capacity and geometric growth,
// plus serde's temporary enum/content representation, at every nesting level.
// One shared owner retains every charge through buffered enum replay as well.
fn charge<T, E: de::Error>(reservation: &Charge) -> Result<(), E> {
    reservation
        .borrow_mut()
        .grow(
            std::mem::size_of::<T>()
                .checked_mul(8)
                .ok_or_else(|| E::custom("Semantic item size overflow"))?,
        )
        .map_err(E::custom)
}
struct Sequence<A> {
    inner: A,
    reservation: Charge,
}
impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Sequence<A> {
    type Error = A::Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        charge::<T::Value, Self::Error>(&self.reservation)?;
        self.inner
            .next_element_seed(Seed(seed, Rc::clone(&self.reservation)))
    }
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
struct Mapping<A> {
    inner: A,
    reservation: Charge,
}
impl<'de, A: MapAccess<'de>> MapAccess<'de> for Mapping<A> {
    type Error = A::Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        charge::<K::Value, Self::Error>(&self.reservation)?;
        self.inner
            .next_key_seed(Seed(seed, Rc::clone(&self.reservation)))
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        charge::<V::Value, Self::Error>(&self.reservation)?;
        self.inner
            .next_value_seed(Seed(seed, Rc::clone(&self.reservation)))
    }
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
macro_rules! scalar {
    ($($name:ident:$ty:ty),*)=>{$(fn $name<E:de::Error>(self,v:$ty)->Result<Self::Value,E>{self.0.$name(v)})*}
}
impl<'de, V: Visitor<'de>> Visitor<'de> for Checked<V> {
    type Value = V::Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.expecting(f)
    }
    scalar!(visit_bool:bool,visit_i8:i8,visit_i16:i16,visit_i32:i32,visit_i64:i64,visit_i128:i128,visit_u8:u8,visit_u16:u16,visit_u32:u32,visit_u64:u64,visit_u128:u128,visit_f32:f32,visit_f64:f64,visit_char:char,visit_string:String,visit_byte_buf:Vec<u8>);
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.0.visit_str(v)
    }
    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
        self.0.visit_borrowed_str(v)
    }
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        self.0.visit_bytes(v)
    }
    fn visit_borrowed_bytes<E: de::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        self.0.visit_borrowed_bytes(v)
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.visit_none()
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.visit_unit()
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.visit_some(Bounded(d, self.1))
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.0.visit_newtype_struct(Bounded(d, self.1))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.0.visit_seq(Sequence {
            inner,
            reservation: self.1,
        })
    }
    fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        // Internally tagged/untagged serde enums replay a buffered Content tree
        // without our adapter. Retain a conservative object expansion allowance
        // for that replay, including omitted defaults, until the whole parse ends.
        let object_bytes = std::mem::size_of::<LevelDef>()
            .saturating_add(std::mem::size_of::<crate::engine::level::PropDef>())
            .saturating_add(std::mem::size_of::<crate::engine::level::ActionDef>());
        self.1
            .borrow_mut()
            .grow(object_bytes.saturating_mul(2))
            .map_err(de::Error::custom)?;
        self.0.visit_map(Mapping {
            inner,
            reservation: self.1,
        })
    }
    fn visit_enum<A: de::EnumAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.0.visit_enum(Enumeration(inner, self.1))
    }
}
struct Enumeration<A>(A, Charge);
struct Variant<A>(A, Charge);
impl<'de, A: de::EnumAccess<'de>> de::EnumAccess<'de> for Enumeration<A> {
    type Error = A::Error;
    type Variant = Variant<A::Variant>;
    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let (v, a) = self.0.variant_seed(Seed(seed, Rc::clone(&self.1)))?;
        Ok((v, Variant(a, self.1)))
    }
}
impl<'de, A: de::VariantAccess<'de>> de::VariantAccess<'de> for Variant<A> {
    type Error = A::Error;
    fn unit_variant(self) -> Result<(), Self::Error> {
        self.0.unit_variant()
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        self.0.newtype_variant_seed(Seed(seed, self.1))
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, v: V) -> Result<V::Value, Self::Error> {
        self.0.tuple_variant(len, Checked(v, self.1))
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.struct_variant(fields, Checked(v, self.1))
    }
}
macro_rules! delegate {
    ($($name:ident $(($($arg:ident:$ty:ty),*))?),*)=>{$(
        fn $name<V:Visitor<'de>>(self,$($($arg:$ty,)*)?v:V)->Result<V::Value,Self::Error>{self.0.$name($($($arg,)*)?Checked(v, self.1))}
    )*}
}
impl<'de, D: Deserializer<'de>> Deserializer<'de> for Bounded<D> {
    type Error = D::Error;
    delegate!(
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
        deserialize_unit_struct(name: &'static str),
        deserialize_newtype_struct(name: &'static str),
        deserialize_seq,
        deserialize_tuple(len: usize),
        deserialize_tuple_struct(name: &'static str, len: usize),
        deserialize_map,
        deserialize_struct(name: &'static str, fields: &'static [&'static str]),
        deserialize_enum(name: &'static str, variants: &'static [&'static str]),
        deserialize_identifier,
        deserialize_ignored_any
    );
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

#[cfg(test)]
#[path = "../tests/semantic_admission.rs"]
mod tests;
