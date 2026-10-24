//! The main window's layout (modules in zones around the main view), kept in the settings so
//! it survives like the rest of them and so the player knows whether the level meter shows.
//! The frontend's `src/layout/model.ts` is the reference; this mirrors its `normalizeLayout`
//! so a hand-edited or older settings file still gives a usable layout.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Module {
    Sidebar,
    Queue,
    Lyrics,
    Artwork,
    Controls,
    Level,
}

impl Module {
    pub const ALL: [Module; 6] = [
        Module::Sidebar,
        Module::Queue,
        Module::Lyrics,
        Module::Artwork,
        Module::Controls,
        Module::Level,
    ];

    fn parse(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }

    /// Columns hold tall modules, bars wide ones; the level meter fits anywhere.
    fn fits(self, zone: Zone) -> bool {
        match self {
            Module::Controls => matches!(zone, Zone::Top | Zone::Bottom),
            Module::Level => true,
            _ => matches!(zone, Zone::Left | Zone::Right),
        }
    }

    /// Where it is by default, and whether it shows there.
    fn home(self) -> (Zone, bool) {
        match self {
            Module::Sidebar => (Zone::Left, true),
            Module::Queue | Module::Lyrics | Module::Artwork => (Zone::Right, false),
            Module::Controls => (Zone::Bottom, true),
            Module::Level => (Zone::Bottom, false),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    const ALL: [Zone; 4] = [Zone::Left, Zone::Right, Zone::Top, Zone::Bottom];

    fn key(self) -> &'static str {
        match self {
            Zone::Left => "left",
            Zone::Right => "right",
            Zone::Top => "top",
            Zone::Bottom => "bottom",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Zones {
    pub left: Vec<Module>,
    pub right: Vec<Module>,
    pub top: Vec<Module>,
    pub bottom: Vec<Module>,
}

impl Zones {
    fn get_mut(&mut self, zone: Zone) -> &mut Vec<Module> {
        match zone {
            Zone::Left => &mut self.left,
            Zone::Right => &mut self.right,
            Zone::Top => &mut self.top,
            Zone::Bottom => &mut self.bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Widths {
    pub left: f64,
    pub right: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SidebarMode {
    #[default]
    Expanded,
    Rail,
}

const COLUMN_WIDTH: (f64, f64) = (180.0, 560.0);
const DEFAULT_WIDTHS: Widths = Widths {
    left: 232.0,
    right: 320.0,
};
const WEIGHT: (f64, f64) = (0.25, 4.0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "Value")]
pub struct Layout {
    pub zones: Zones,
    pub hidden: Vec<Module>,
    pub widths: Widths,
    pub weights: BTreeMap<Module, f64>,
    pub sidebar: SidebarMode,
}

impl Default for Layout {
    fn default() -> Self {
        Layout::from(Value::Null)
    }
}

impl Layout {
    /// Whether a module is on screen (every module is placed somewhere).
    pub fn shows(&self, module: Module) -> bool {
        !self.hidden.contains(&module)
    }
}

/// Any value becomes a usable layout, as `normalizeLayout` does in the frontend: unknown
/// modules and repeats are dropped, a module in a zone it doesn't fit goes home (still
/// shown), modules never mentioned join their home zone as they are by default, and sizes
/// are brought within limits.
impl From<Value> for Layout {
    fn from(value: Value) -> Self {
        let zones_in = value.get("zones");
        let mut zones = Zones::default();
        let mut placed = BTreeSet::new();
        let mut misplaced = Vec::new();
        for zone in Zone::ALL {
            let items = zones_in
                .and_then(|zones| zones.get(zone.key()))
                .and_then(Value::as_array);
            for module in items.into_iter().flatten().filter_map(Module::parse) {
                if !placed.insert(module) {
                    continue;
                }
                if module.fits(zone) {
                    zones.get_mut(zone).push(module);
                } else {
                    misplaced.push(module);
                }
            }
        }

        let mut hidden: BTreeSet<Module> = value
            .get("hidden")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Module::parse)
            .collect();
        for module in Module::ALL {
            if !placed.contains(&module) {
                let (zone, shown) = module.home();
                zones.get_mut(zone).push(module);
                if !shown {
                    hidden.insert(module);
                }
            }
        }
        for module in misplaced {
            zones.get_mut(module.home().0).push(module);
        }

        let width = |key: &str, default: f64| {
            value
                .get("widths")
                .and_then(|widths| widths.get(key))
                .and_then(Value::as_f64)
                .filter(|width| width.is_finite())
                .map_or(default, |width| {
                    width.round().clamp(COLUMN_WIDTH.0, COLUMN_WIDTH.1)
                })
        };
        let widths = Widths {
            left: width("left", DEFAULT_WIDTHS.left),
            right: width("right", DEFAULT_WIDTHS.right),
        };

        let mut weights = BTreeMap::new();
        if let Some(Value::Object(map)) = value.get("weights") {
            for (key, weight) in map {
                let module = Module::parse(&Value::String(key.clone()));
                let weight = weight.as_f64().filter(|w| w.is_finite() && *w != 1.0);
                if let (Some(module), Some(weight)) = (module, weight) {
                    weights.insert(module, weight.clamp(WEIGHT.0, WEIGHT.1));
                }
            }
        }

        let sidebar = match value.get("sidebar").and_then(Value::as_str) {
            Some("rail") => SidebarMode::Rail,
            _ => SidebarMode::Expanded,
        };

        Layout {
            zones,
            hidden: hidden.into_iter().collect(),
            widths,
            weights,
            sidebar,
        }
    }
}

/// A layout saved under a name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutPreset {
    pub name: String,
    pub layout: Layout,
}

/// The most presets kept, and the longest name.
const MAX_PRESETS: usize = 20;
const MAX_NAME: usize = 40;

/// Presets with a name, each name once (the first wins), at most [`MAX_PRESETS`].
pub fn sanitize_presets(presets: Vec<LayoutPreset>) -> Vec<LayoutPreset> {
    let mut kept: Vec<LayoutPreset> = Vec::new();
    for mut preset in presets {
        let name: String = preset.name.trim().chars().take(MAX_NAME).collect();
        if name.is_empty() || kept.iter().any(|known| known.name == name) {
            continue;
        }
        preset.name = name;
        kept.push(preset);
        if kept.len() == MAX_PRESETS {
            break;
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn layout(value: Value) -> Layout {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_default_is_the_classic_window() {
        let layout = Layout::default();
        assert_eq!(layout.zones.left, [Module::Sidebar]);
        assert_eq!(
            layout.zones.right,
            [Module::Queue, Module::Lyrics, Module::Artwork]
        );
        assert_eq!(layout.zones.bottom, [Module::Controls, Module::Level]);
        assert!(layout.shows(Module::Sidebar) && layout.shows(Module::Controls));
        assert!(!layout.shows(Module::Level));
        // It goes out in the shape the frontend reads, and comes back the same.
        let json = serde_json::to_value(&layout).unwrap();
        assert_eq!(json["zones"]["left"], json!(["sidebar"]));
        assert_eq!(json["widths"], json!({ "left": 232.0, "right": 320.0 }));
        assert_eq!(json["sidebar"], json!("expanded"));
        assert_eq!(serde_json::from_value::<Layout>(json).unwrap(), layout);
    }

    #[test]
    fn every_module_is_placed_once_where_it_fits() {
        let layout = layout(json!({
            "zones": {
                "left": ["controls", "sidebar", "nope", "sidebar"],
                "top": ["queue", "level"],
                "right": ["lyrics"]
            },
            "hidden": ["artwork", 7]
        }));
        assert_eq!(layout.zones.left, [Module::Sidebar]);
        assert_eq!(layout.zones.top, [Module::Level]);
        assert_eq!(
            layout.zones.right,
            [Module::Lyrics, Module::Artwork, Module::Queue]
        );
        assert_eq!(layout.zones.bottom, [Module::Controls]);
        // Misplaced modules stay shown at home; never-mentioned ones come as by default.
        assert!(layout.shows(Module::Queue) && layout.shows(Module::Controls));
        assert_eq!(layout.hidden, [Module::Artwork]);
    }

    #[test]
    fn sizes_stay_within_limits() {
        let layout = layout(json!({
            "widths": { "left": 3, "right": "wide" },
            "weights": { "queue": 99, "lyrics": 0.5, "artwork": 1, "nope": 2 },
            "sidebar": "rail"
        }));
        assert_eq!(layout.widths.left, COLUMN_WIDTH.0);
        assert_eq!(layout.widths.right, DEFAULT_WIDTHS.right);
        assert_eq!(
            layout.weights,
            BTreeMap::from([(Module::Queue, 4.0), (Module::Lyrics, 0.5)])
        );
        assert_eq!(layout.sidebar, SidebarMode::Rail);
    }

    #[test]
    fn presets_need_distinct_names() {
        let preset = |name: &str| LayoutPreset {
            name: name.into(),
            layout: Layout::default(),
        };
        let kept = sanitize_presets(vec![
            preset(" Focus "),
            preset(""),
            preset("Focus"),
            preset("Wide"),
        ]);
        let names: Vec<&str> = kept.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Focus", "Wide"]);
        let many = (0..30).map(|i| preset(&format!("P{i}"))).collect();
        assert_eq!(sanitize_presets(many).len(), MAX_PRESETS);
    }
}
