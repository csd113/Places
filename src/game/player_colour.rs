//! Saved sRGB player colour, independent of maps and shared mannequin assets.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerColour([u8; 3]);

const PRESETS: [(PlayerColour, &str); 8] = [
    (PlayerColour([0x4f, 0x77, 0x74]), "Muted teal"),
    (PlayerColour([0x57, 0x6e, 0x8a]), "Slate blue"),
    (PlayerColour([0xa0, 0x68, 0x54]), "Terracotta"),
    (PlayerColour([0x78, 0x80, 0x53]), "Olive"),
    (PlayerColour([0xb4, 0xa3, 0x7c]), "Sand"),
    (PlayerColour([0x85, 0x64, 0x7e]), "Plum"),
    (PlayerColour([0x49, 0x4d, 0x50]), "Graphite"),
    (PlayerColour([0xd1, 0xcc, 0xbc]), "Ivory"),
];

impl Default for PlayerColour {
    fn default() -> Self {
        Self([0x4f, 0x77, 0x74])
    }
}

impl PlayerColour {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#').unwrap_or(value);
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |start, end| u8::from_str_radix(hex.get(start..end)?, 16).ok();
        Some(Self([channel(0, 2)?, channel(2, 4)?, channel(4, 6)?]))
    }

    #[must_use]
    pub fn hex(self) -> String {
        let [red, green, blue] = self.0;
        format!("#{red:02X}{green:02X}{blue:02X}")
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        PRESETS
            .iter()
            .find(|(colour, _)| *colour == self)
            .map_or("Custom", |(_, label)| *label)
    }

    #[must_use]
    pub fn linear_rgb(self) -> [f32; 3] {
        self.0.map(crate::engine::materials::color::decode_byte)
    }

    #[must_use]
    pub fn step(self, direction: i32) -> Self {
        let index = PRESETS.iter().position(|(colour, _)| *colour == self);
        let next = index.map_or(0, |current| {
            if direction < 0_i32 {
                current
                    .checked_sub(1)
                    .unwrap_or(PRESETS.len().saturating_sub(1))
            } else {
                current
                    .saturating_add(1)
                    .checked_rem(PRESETS.len())
                    .unwrap_or_default()
            }
        });
        PRESETS
            .get(next)
            .map_or_else(Self::default, |(colour, _)| *colour)
    }
}

impl Serialize for PlayerColour {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}
impl<'de> Deserialize<'de> for PlayerColour {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value).unwrap_or_default())
    }
}
