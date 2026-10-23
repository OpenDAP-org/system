//! The OpenDAP device description (`opendap.device-description/v1`, hardware support spec §3):
//! what devicetree does not say about a device, as data. Only the sections the shell and the
//! product host read so far are modelled: identity, display and controls.
//!
//! Nothing here knows a board. Surfaces see control **roles**, never Linux key codes, and
//! physical sizes, never a fixed resolution (hardware support spec §9).

use serde::Deserialize;

pub const SCHEMA: &str = "opendap.device-description/v1";

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Description {
    pub schema: String,
    pub identity: Identity,
    pub display: Option<Display>,
    #[serde(default)]
    pub controls: Vec<Control>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub kind: Kind,
    pub id: String,
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Board,
    Component,
    Faceplate,
    Hat,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Display {
    pub width_px: u32,
    pub height_px: u32,
    pub width_mm: f32,
    pub height_mm: f32,
    pub shape: Shape,
    pub technology: Technology,
    pub distance: DistanceClass,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Rectangle,
    Rounded,
    Round,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Technology {
    Lcd,
    Oled,
    EPaper,
    Monochrome,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DistanceClass {
    Handheld,
    Desk,
}

impl DistanceClass {
    /// Typical reading distance in centimetres (design system §18.2).
    pub fn reading_cm(self) -> f32 {
        match self {
            DistanceClass::Handheld => 35.0,
            DistanceClass::Desk => 60.0,
        }
    }
}

/// The design pixel: 0.153 mm seen from 35 cm (design system §18.2, DS-022).
pub const DESIGN_PX_MM: f32 = 0.153;

impl Display {
    pub fn px_per_mm(&self) -> f32 {
        self.width_px as f32 / self.width_mm
    }

    /// Physical pixels per design pixel: (px/mm × 0.153) × (distance / 35 cm).
    pub fn scale(&self) -> f32 {
        self.px_per_mm() * DESIGN_PX_MM * (self.distance.reading_cm() / 35.0)
    }

    pub fn mm_to_px(&self, mm: f32) -> f32 {
        mm * self.px_per_mm()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub role: Role,
    pub kind: ControlKind,
    /// The Linux input code its driver produces, by name (`KEY_VOLUMEUP`).
    pub code: String,
    pub edge: Edge,
    /// Centre of the control along its edge, from the top (or start) of the display's visible area.
    pub offset_mm: f32,
    pub length_mm: f32,
}

/// What a control means. The UI and capabilities act on roles only.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Action,
    VolumeUp,
    VolumeDown,
    Main,
    Back,
    Select,
    Menu,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ControlKind {
    Button,
    RockerHalf,
    Dial,
    Slider,
    Switch,
}

/// The physical edge of the enclosure a control sits on, as the listener holds the device.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a device description: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unsupported schema {found:?}, expected {SCHEMA:?}")]
    Schema { found: String },
    #[error("two controls claim the code {0}")]
    DuplicateCode(String),
}

impl Description {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let d: Description = toml::from_str(text)?;
        if d.schema != SCHEMA {
            return Err(Error::Schema { found: d.schema });
        }
        let mut codes: Vec<&str> = d.controls.iter().map(|c| c.code.as_str()).collect();
        codes.sort_unstable();
        if let Some(w) = codes.windows(2).find(|w| w[0] == w[1]) {
            return Err(Error::DuplicateCode(w[0].to_string()));
        }
        Ok(d)
    }

    /// The role a Linux input code stands for on this device, if any.
    pub fn role_of(&self, code: &str) -> Option<Role> {
        self.controls.iter().find(|c| c.code == code).map(|c| c.role)
    }

    pub fn control(&self, role: Role) -> Option<&Control> {
        self.controls.iter().find(|c| c.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIRTUAL_T1: &str = include_str!("../fixtures/virtual-t1.toml");
    const NO_DISPLAY: &str = include_str!("../fixtures/no-display.toml");

    #[test]
    fn virtual_t1_maps_codes_to_roles() {
        let d = Description::parse(VIRTUAL_T1).unwrap();
        assert_eq!(d.role_of("KEY_PLAYPAUSE"), Some(Role::Action));
        assert_eq!(d.role_of("KEY_VOLUMEUP"), Some(Role::VolumeUp));
        assert_eq!(d.role_of("KEY_VOLUMEDOWN"), Some(Role::VolumeDown));
        assert_eq!(d.role_of("KEY_POWER"), Some(Role::Main));
        assert_eq!(d.role_of("KEY_A"), None);
    }

    #[test]
    fn virtual_t1_is_one_design_pixel_per_pixel() {
        let d = Description::parse(VIRTUAL_T1).unwrap();
        let s = d.display.unwrap().scale();
        assert!((s - 1.0).abs() < 0.01, "scale {s}");
    }

    #[test]
    fn a_board_without_display_is_valid() {
        let d = Description::parse(NO_DISPLAY).unwrap();
        assert!(d.display.is_none());
        assert_eq!(d.role_of("KEY_PLAYPAUSE"), Some(Role::Action));
    }

    #[test]
    fn rejects_other_schemas_and_duplicate_codes() {
        let other = VIRTUAL_T1.replace(SCHEMA, "opendap.device-description/v9");
        assert!(matches!(Description::parse(&other), Err(Error::Schema { .. })));
        let dup = VIRTUAL_T1.replace("KEY_VOLUMEDOWN", "KEY_VOLUMEUP");
        assert!(matches!(Description::parse(&dup), Err(Error::DuplicateCode(_))));
    }
}
