use serde::{Deserialize, Serialize};

use crate::terrain::BaseTerrain;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Exposed,   // Tundra, Desert, Grasslands - hard to hide
    Moderate,  // Clearing, Highlands, Wetlands
    Concealed, // Forest, Jungle, UrbanRuins - easy to hide
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Harshness {
    Mild,     // Clearing, Grasslands
    Moderate, // Forest, Jungle, UrbanRuins, Wetlands, Highlands, Geothermal
    Harsh,    // Desert, Tundra, Mountains, Badlands
}

#[derive(Debug, Clone, Copy)]
pub struct ItemWeights {
    pub weapons: f32,
    pub shields: f32,
    /// Armor spawn slice. WHY: carved from `consumables` as a fixed 0.1
    /// everywhere — keeps every terrain's weapon/shield ratios (and the
    /// distribution tests that pin them) untouched; per-terrain tuning
    /// comes from playtest, not here.
    pub armor: f32,
    pub consumables: f32,
}

impl BaseTerrain {
    pub const fn movement_cost(&self) -> f32 {
        match self {
            BaseTerrain::Clearing => 1.0,
            BaseTerrain::Grasslands => 0.9,
            BaseTerrain::UrbanRuins => 1.2,
            BaseTerrain::Forest => 1.3,
            BaseTerrain::Jungle => 1.4,
            BaseTerrain::Geothermal => 1.4,
            BaseTerrain::Wetlands => 1.5,
            BaseTerrain::Highlands => 1.6,
            BaseTerrain::Badlands => 1.7,
            BaseTerrain::Mountains => 1.8,
            BaseTerrain::Desert => 2.0,
            BaseTerrain::Tundra => 2.0,
        }
    }

    pub const fn visibility(&self) -> Visibility {
        match self {
            BaseTerrain::Forest | BaseTerrain::Jungle | BaseTerrain::UrbanRuins => {
                Visibility::Concealed
            }
            BaseTerrain::Desert
            | BaseTerrain::Tundra
            | BaseTerrain::Grasslands
            | BaseTerrain::Badlands => Visibility::Exposed,
            _ => Visibility::Moderate,
        }
    }

    pub const fn harshness(&self) -> Harshness {
        match self {
            BaseTerrain::Clearing | BaseTerrain::Grasslands => Harshness::Mild,
            BaseTerrain::Desert
            | BaseTerrain::Tundra
            | BaseTerrain::Mountains
            | BaseTerrain::Badlands => Harshness::Harsh,
            _ => Harshness::Moderate,
        }
    }

    pub const fn item_spawn_modifier(&self) -> f32 {
        match self {
            BaseTerrain::Clearing => 1.0,
            BaseTerrain::Jungle => 1.0,
            BaseTerrain::Forest => 1.1,
            BaseTerrain::Grasslands => 1.1,
            BaseTerrain::UrbanRuins => 1.2,
            BaseTerrain::Wetlands => 0.9,
            BaseTerrain::Highlands => 0.8,
            BaseTerrain::Geothermal => 0.8,
            BaseTerrain::Mountains => 0.7,
            BaseTerrain::Badlands => 0.7,
            BaseTerrain::Desert => 0.6,
            BaseTerrain::Tundra => 0.6,
        }
    }

    pub const fn item_weights(&self) -> ItemWeights {
        match self {
            BaseTerrain::Desert => ItemWeights {
                weapons: 0.2,
                shields: 0.2,
                armor: 0.1,
                consumables: 0.5,
            },
            BaseTerrain::Tundra => ItemWeights {
                weapons: 0.3,
                shields: 0.4,
                armor: 0.1,
                consumables: 0.2,
            },
            BaseTerrain::UrbanRuins => ItemWeights {
                weapons: 0.5,
                shields: 0.3,
                armor: 0.1,
                consumables: 0.1,
            },
            BaseTerrain::Forest => ItemWeights {
                weapons: 0.3,
                shields: 0.2,
                armor: 0.1,
                consumables: 0.4,
            },
            BaseTerrain::Mountains => ItemWeights {
                weapons: 0.4,
                shields: 0.4,
                armor: 0.1,
                consumables: 0.1,
            },
            BaseTerrain::Wetlands => ItemWeights {
                weapons: 0.25,
                shields: 0.25,
                armor: 0.1,
                consumables: 0.4,
            },
            BaseTerrain::Jungle => ItemWeights {
                weapons: 0.2,
                shields: 0.3,
                armor: 0.1,
                consumables: 0.4,
            },
            BaseTerrain::Clearing => ItemWeights {
                weapons: 0.33,
                shields: 0.33,
                armor: 0.1,
                consumables: 0.24,
            },
            BaseTerrain::Grasslands => ItemWeights {
                weapons: 0.30,
                shields: 0.30,
                armor: 0.1,
                consumables: 0.30,
            },
            BaseTerrain::Badlands => ItemWeights {
                weapons: 0.35,
                shields: 0.30,
                armor: 0.1,
                consumables: 0.25,
            },
            BaseTerrain::Highlands => ItemWeights {
                weapons: 0.30,
                shields: 0.35,
                armor: 0.1,
                consumables: 0.25,
            },
            BaseTerrain::Geothermal => ItemWeights {
                weapons: 0.30,
                shields: 0.30,
                armor: 0.1,
                consumables: 0.30,
            },
        }
    }
}
