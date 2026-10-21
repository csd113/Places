//! Explicit developer-give pickup choices, reviewed against the shipped catalog.
//!
//! Keep listing and execution on this same list. Asset names, categories, static
//! solidity and the presence of a reusable physics profile never grant access.
//! Physical tuning remains in `assets/gameplay.json`; visual assets are unchanged.
//! See `docs/reports/release-0.11.0.md#console-pickup-audit-0110` for the complete audit.

/// One stable catalog identity approved for developer-given pickup gameplay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickupSpec {
    pub id: &'static str,
    pub name: &'static str,
}

/// Reviewed choices in stable catalog-id order, including each printed variant.
/// The lit cake retains its embedded flame clip through the animated-item path.
pub const ITEMS: &[PickupSpec] = &[
    PickupSpec {
        id: "arcade:bowling_ball_black",
        name: "Bowling Ball Black",
    },
    PickupSpec {
        id: "arcade:bowling_ball_burgundy",
        name: "Bowling Ball Burgundy",
    },
    PickupSpec {
        id: "arcade:bowling_ball_cobalt",
        name: "Bowling Ball Cobalt",
    },
    PickupSpec {
        id: "arcade:bowling_ball_green",
        name: "Bowling Ball Green",
    },
    PickupSpec {
        id: "arcade:bowling_ball_marbled",
        name: "Bowling Ball Marbled",
    },
    PickupSpec {
        id: "arcade:bowling_ball_purple",
        name: "Bowling Ball Purple",
    },
    PickupSpec {
        id: "arcade:bowling_ball_red",
        name: "Bowling Ball Red",
    },
    PickupSpec {
        id: "arcade:bowling_pin",
        name: "Bowling Pin",
    },
    PickupSpec {
        id: "arcade:cat_plush",
        name: "Tuxedo Cat Plush",
    },
    PickupSpec {
        id: "arcade:litter_bottle_fallen",
        name: "Litter Bottle Fallen",
    },
    PickupSpec {
        id: "arcade:litter_can",
        name: "Litter Can",
    },
    PickupSpec {
        id: "arcade:litter_cup",
        name: "Litter Cup",
    },
    PickupSpec {
        id: "arcade:litter_paper",
        name: "Litter Paper",
    },
    PickupSpec {
        id: "arcade:litter_wrapper",
        name: "Litter Wrapper",
    },
    PickupSpec {
        id: "arcade:party_balloon_weight",
        name: "Party Balloon Weight",
    },
    PickupSpec {
        id: "arcade:party_cake_lit",
        name: "Party Cake Lit",
    },
    PickupSpec {
        id: "arcade:party_cake_unlit",
        name: "Party Cake Unlit",
    },
    PickupSpec {
        id: "arcade:party_cup_balloons",
        name: "Party Cup Balloons",
    },
    PickupSpec {
        id: "arcade:party_cup_confetti",
        name: "Party Cup Confetti",
    },
    PickupSpec {
        id: "arcade:party_cup_geometric",
        name: "Party Cup Geometric",
    },
    PickupSpec {
        id: "arcade:party_cup_stars",
        name: "Party Cup Stars",
    },
    PickupSpec {
        id: "arcade:party_gift_arcade",
        name: "Party Gift Arcade",
    },
    PickupSpec {
        id: "arcade:party_gift_confetti",
        name: "Party Gift Confetti",
    },
    PickupSpec {
        id: "arcade:party_gift_green",
        name: "Party Gift Green",
    },
    PickupSpec {
        id: "arcade:party_gift_red",
        name: "Party Gift Red",
    },
    PickupSpec {
        id: "arcade:party_gift_stars",
        name: "Party Gift Stars",
    },
    PickupSpec {
        id: "arcade:party_gift_stripes",
        name: "Party Gift Stripes",
    },
    PickupSpec {
        id: "arcade:party_hat_confetti",
        name: "Party Hat Confetti",
    },
    PickupSpec {
        id: "arcade:party_hat_dots",
        name: "Party Hat Dots",
    },
    PickupSpec {
        id: "arcade:party_hat_shapes",
        name: "Party Hat Shapes",
    },
    PickupSpec {
        id: "arcade:party_hat_stars",
        name: "Party Hat Stars",
    },
    PickupSpec {
        id: "arcade:party_hat_stripes",
        name: "Party Hat Stripes",
    },
    PickupSpec {
        id: "arcade:party_plate_balloons",
        name: "Party Plate Balloons",
    },
    PickupSpec {
        id: "arcade:party_plate_border",
        name: "Party Plate Border",
    },
    PickupSpec {
        id: "arcade:party_plate_confetti",
        name: "Party Plate Confetti",
    },
    PickupSpec {
        id: "arcade:party_plate_stars",
        name: "Party Plate Stars",
    },
    PickupSpec {
        id: "arcade:soda_bottle",
        name: "Soda Bottle",
    },
    PickupSpec {
        id: "beach:crate",
        name: "Beach Crate",
    },
    PickupSpec {
        id: "core:cardboard_box",
        name: "Cardboard Box",
    },
    PickupSpec {
        id: "core:crate",
        name: "Crate",
    },
    PickupSpec {
        id: "core:plant",
        name: "Plant",
    },
    PickupSpec {
        id: "core:rubber_duck",
        name: "Rubber Duck",
    },
    PickupSpec {
        id: "core:tv",
        name: "Television",
    },
    PickupSpec {
        id: "core:washer_drum",
        name: "Washer Drum",
    },
    PickupSpec {
        id: "home:book_stack",
        name: "Two Stacked Books",
    },
    PickupSpec {
        id: "home:bowl",
        name: "Cereal Bowl",
    },
    PickupSpec {
        id: "home:crt_tv",
        name: "CRT Television",
    },
    PickupSpec {
        id: "home:cushion",
        name: "Checked Throw Cushion",
    },
    PickupSpec {
        id: "home:fork",
        name: "Table Fork",
    },
    PickupSpec {
        id: "home:kettle",
        name: "Home Kettle",
    },
    PickupSpec {
        id: "home:knife",
        name: "Table Knife",
    },
    PickupSpec {
        id: "home:landscape_frame",
        name: "Small Framed Landscape",
    },
    PickupSpec {
        id: "home:mug",
        name: "Ceramic Coffee Mug",
    },
    PickupSpec {
        id: "home:plant_table",
        name: "Small Potted Plant",
    },
    PickupSpec {
        id: "home:plate",
        name: "Dinner Plate",
    },
    PickupSpec {
        id: "home:spoon",
        name: "Table Spoon",
    },
    PickupSpec {
        id: "home:toaster",
        name: "Home Toaster",
    },
    PickupSpec {
        id: "pool:pool_potted_plant",
        name: "Pool Potted Plant",
    },
];

/// Resolves only an exact reviewed catalog id; aliases belong to command parsing.
#[must_use]
pub fn find(id: &str) -> Option<&'static PickupSpec> {
    ITEMS.iter().find(|item| item.id == id)
}

/// Runtime illumination belonging to a complete portable model.
/// The cake's seven flame meshes and flicker clip stay inside its shared GLB.
#[must_use]
pub fn runtime_glow(id: &str) -> Option<crate::engine::entity::GlowCue> {
    (id == "arcade:party_cake_lit").then_some(crate::engine::entity::GlowCue {
        socket: None,
        offset: [0.0, 0.218, 0.0],
        color: [1.0, 0.49, 0.16],
        intensity: 0.28,
        range: 0.8,
        fade_with_opacity: false,
    })
}
