use super::*;
use crate::items::name_generator::{generate_shield_name_with_rng, generate_weapon_name_with_rng};
use crate::terrain::BaseTerrain;
use rand::RngExt;
use rand::prelude::*;
use strum::IntoEnumIterator;

impl ItemRarity {
    /// Roll for item rarity using weighted distribution.
    pub fn random() -> ItemRarity {
        Self::random_with_rng(&mut rand::rng())
    }

    /// Roll for item rarity using weighted distribution, drawing every
    /// random value from `rng` so callers can reproduce the roll.
    pub fn random_with_rng(rng: &mut impl Rng) -> ItemRarity {
        let roll: f32 = rng.random();

        if roll < 0.60 {
            ItemRarity::Common
        } else if roll < 0.85 {
            ItemRarity::Uncommon
        } else if roll < 0.97 {
            ItemRarity::Rare
        } else {
            ItemRarity::Legendary
        }
    }
}

impl ItemType {
    pub fn random() -> ItemType {
        Self::random_with_rng(&mut rand::rng())
    }

    /// Pick a random item type from `rng`.
    pub fn random_with_rng(rng: &mut impl Rng) -> ItemType {
        // Weighted distribution: weapons and consumables remain dominant;
        // food and water enter the spawn pool but are rarer (spec).
        match rng.random_range(0..10) {
            0..=3 => ItemType::Consumable,
            4..=6 => ItemType::Weapon,
            7..=8 => ItemType::Food(rng.random_range(1..=5)),
            _ => ItemType::Water(rng.random_range(1..=3)),
        }
    }
}

impl Attribute {
    pub fn random() -> Attribute {
        Self::random_with_rng(&mut rand::rng())
    }

    /// Pick a random attribute from `rng`.
    pub fn random_with_rng(rng: &mut impl Rng) -> Attribute {
        Attribute::iter().choose(rng).unwrap()
    }
}

impl Item {
    pub fn new_random(name: Option<&str>) -> Item {
        Self::new_random_with_rng(name, &mut rand::rng())
    }

    /// Seedable variant of [`Item::new_random`]: every random choice is
    /// drawn from `rng`, so a fixed seed reproduces the same item.
    pub fn new_random_with_rng(name: Option<&str>, rng: &mut impl Rng) -> Item {
        let item_type = ItemType::random_with_rng(rng);
        let is_shield = rng.random_bool(0.5);

        match (item_type, name) {
            (ItemType::Consumable, Some(name)) => Self::new_consumable_with_rng(name, rng),
            (ItemType::Consumable, None) => Self::new_random_consumable_with_rng(rng),
            (ItemType::Weapon, Some(name)) => match is_shield {
                false => Self::new_weapon_with_rng(name, rng),
                true => Self::new_shield_with_rng(name, rng),
            },
            (ItemType::Weapon, None) => match is_shield {
                false => Self::new_random_weapon_with_rng(rng),
                true => Self::new_random_shield_with_rng(rng),
            },
            (ItemType::Food(n), name) => Self::new_food(name, n),
            (ItemType::Water(n), name) => Self::new_water(name, n),
        }
    }

    /// Create a random item using terrain-based weights for item type distribution.
    ///
    /// Uses the terrain's `item_weights()` to determine the probability of creating
    /// weapons, shields, or consumables, making item spawning terrain-appropriate.
    ///
    /// # Arguments
    /// * `terrain` - The terrain type that influences item distribution
    /// * `name` - Optional item name; generates one if None
    ///
    /// # Example
    /// ```
    /// use world::items::Item;
    /// use world::terrain::BaseTerrain;
    ///
    /// // Desert terrain favors consumables (0.6 weight)
    /// let item = Item::new_random_with_terrain(BaseTerrain::Desert, None);
    ///
    /// // Urban ruins favor weapons (0.5 weight)
    /// let item = Item::new_random_with_terrain(BaseTerrain::UrbanRuins, None);
    /// ```
    pub fn new_random_with_terrain(terrain: BaseTerrain, name: Option<&str>) -> Item {
        Self::new_random_with_terrain_and_rng(terrain, name, &mut rand::rng())
    }

    /// Seedable variant of [`Item::new_random_with_terrain`]: the terrain roll
    /// and every item detail are drawn from `rng`, so a fixed seed reproduces
    /// the same sequence of items.
    pub fn new_random_with_terrain_and_rng(
        terrain: BaseTerrain,
        name: Option<&str>,
        rng: &mut impl Rng,
    ) -> Item {
        let weights = terrain.item_weights();

        // Use weighted random selection based on terrain
        let roll: f32 = rng.random();

        if roll < weights.weapons {
            // Generate weapon
            match name {
                Some(n) => Self::new_weapon_with_rng(n, rng),
                None => Self::new_random_weapon_with_rng(rng),
            }
        } else if roll < weights.weapons + weights.shields {
            // Generate shield
            match name {
                Some(n) => Self::new_shield_with_rng(n, rng),
                None => Self::new_random_shield_with_rng(rng),
            }
        } else {
            // Generate consumable
            match name {
                Some(n) => Self::new_consumable_with_rng(n, rng),
                None => Self::new_random_consumable_with_rng(rng),
            }
        }
    }

    pub fn new_weapon(name: &str) -> Item {
        Self::new_weapon_with_rng(name, &mut rand::rng())
    }

    /// Build a named weapon whose rarity and stats come from `rng`.
    pub fn new_weapon_with_rng(name: &str, rng: &mut impl Rng) -> Item {
        let rarity = ItemRarity::random_with_rng(rng);
        let attribute = Attribute::Strength;
        let (min, max) = rarity.effect_range();
        let effect = rng.random_range(min..=max);
        let (dur_min, dur_max) = rarity.weapon_durability_range();
        let durability = rng.random_range(dur_min..=dur_max);

        Item::new(
            name,
            ItemType::Weapon,
            rarity,
            durability,
            attribute,
            effect,
        )
    }

    pub fn new_random_weapon() -> Item {
        Self::new_random_weapon_with_rng(&mut rand::rng())
    }

    /// Build a weapon with a generated name, drawing everything from `rng`.
    pub fn new_random_weapon_with_rng(rng: &mut impl Rng) -> Item {
        let name = generate_weapon_name_with_rng(rng);
        Self::new_weapon_with_rng(name.as_str(), rng)
    }

    pub fn new_consumable(name: &str) -> Item {
        Self::new_consumable_with_rng(name, &mut rand::rng())
    }

    /// Build a named consumable whose rarity and stats come from `rng`.
    pub fn new_consumable_with_rng(name: &str, rng: &mut impl Rng) -> Item {
        let rarity = ItemRarity::random_with_rng(rng);
        let attribute = Attribute::random_with_rng(rng);
        let (min, max) = rarity.effect_range();
        let effect = rng.random_range(min..=max);

        Item::new(name, ItemType::Consumable, rarity, 1, attribute, effect)
    }

    pub fn new_random_consumable() -> Item {
        Self::new_random_consumable_with_rng(&mut rand::rng())
    }

    /// Build a consumable with a generated name, drawing everything from `rng`.
    pub fn new_random_consumable_with_rng(rng: &mut impl Rng) -> Item {
        let rarity = ItemRarity::random_with_rng(rng);
        let attribute = Attribute::random_with_rng(rng);
        let name = attribute.consumable_name();
        let (min, max) = rarity.effect_range();
        let effect = rng.random_range(min..=max);

        Item::new(&name, ItemType::Consumable, rarity, 1, attribute, effect)
    }

    pub fn new_shield(name: &str) -> Item {
        Self::new_shield_with_rng(name, &mut rand::rng())
    }

    /// Build a named shield whose rarity and stats come from `rng`.
    pub fn new_shield_with_rng(name: &str, rng: &mut impl Rng) -> Item {
        let rarity = ItemRarity::random_with_rng(rng);
        let item_type = ItemType::Weapon;
        let attribute = Attribute::Defense;
        let (min, max) = rarity.effect_range();
        let effect = rng.random_range(min..=max);
        let (dur_min, dur_max) = rarity.shield_durability_range();
        let durability = rng.random_range(dur_min..=dur_max);

        Item::new(name, item_type, rarity, durability, attribute, effect)
    }

    pub fn new_random_shield() -> Item {
        Self::new_random_shield_with_rng(&mut rand::rng())
    }

    /// Build a shield with a generated name, drawing everything from `rng`.
    pub fn new_random_shield_with_rng(rng: &mut impl Rng) -> Item {
        let name = generate_shield_name_with_rng(rng);
        Self::new_shield_with_rng(name.as_str(), rng)
    }

    /// Construct a Food item carrying `value` hunger-debt relief. `name` is
    /// optional; if absent, a generic "ration" name is generated.
    pub fn new_food(name: Option<&str>, value: u8) -> Item {
        let display = name.map(|s| s.to_string()).unwrap_or_else(|| {
            // Tiny deterministic label pool keeps generation cheap and the
            // payload self-describing without dragging in the full name
            // generator just yet.
            format!("ration ({})", value)
        });
        Item::new(
            &display,
            ItemType::Food(value),
            ItemRarity::Common,
            1,
            Attribute::Health,
            value as i32,
        )
    }

    /// Construct a Water item carrying `value` thirst-debt relief.
    pub fn new_water(name: Option<&str>, value: u8) -> Item {
        let display = name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("waterskin ({})", value));
        Item::new(
            &display,
            ItemType::Water(value),
            ItemRarity::Common,
            1,
            Attribute::Health,
            value as i32,
        )
    }
}
