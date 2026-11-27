//! Artist-audited metres in each fragment's local frame and bottle assembly.
use super::{BottleKind, Panel};

pub(super) const fn panels(kind: BottleKind) -> &'static [Panel] {
    match kind {
        BottleKind::Beer => &BEER,
        BottleKind::Vodka => &VODKA,
        BottleKind::Wine => &WINE,
        BottleKind::Gin => &GIN,
    }
}

const BEER: [Panel; 5] = [
    Panel {
        path: "environments/home/props/beer_bottle_piece_heel.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.033, 0.0, -0.033],
        max: [0.033, 0.047_227_4, 0.033],
    },
    Panel {
        path: "environments/home/props/beer_bottle_piece_neck.glb",
        offset: [0.0, 0.181, 0.0],
        min: [-0.018_81, 0.003_199_6, -0.018_81],
        max: [0.018_81, 0.05, 0.018_81],
    },
    Panel {
        path: "environments/home/props/beer_bottle_piece_front.glb",
        offset: [0.0, 0.055_024_3, 0.033],
        min: [-0.020_232_9, 0.0, -0.011_399_9],
        max: [0.022_734_4, 0.084_900_7, -0.000_421_5],
    },
    Panel {
        path: "environments/home/props/beer_bottle_piece_back.glb",
        offset: [0.0, 0.068_534_3, -0.033],
        min: [-0.022_734_4, 0.0, 0.001_195_6],
        max: [0.020_232_9, 0.084_900_7, 0.011_399_9],
    },
    Panel {
        path: "environments/home/props/beer_bottle_piece_side.glb",
        offset: [0.032_762_7, 0.089_764_3, -0.003_950_5],
        min: [-0.015_191_1, 0.0, -0.018_629],
        max: [-0.000_491_8, 0.084_900_7, 0.017_836_2],
    },
];

const VODKA: [Panel; 5] = [
    Panel {
        path: "environments/home/props/vodka_bottle_piece_heel.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.037, 0.0, -0.0296],
        max: [0.037, 0.055_227_4, 0.0296],
    },
    Panel {
        path: "environments/home/props/vodka_bottle_piece_neck.glb",
        offset: [0.0, 0.203, 0.0],
        min: [-0.019, 0.003_199_6, -0.019],
        max: [0.019, 0.065, 0.019],
    },
    Panel {
        path: "environments/home/props/vodka_bottle_piece_front.glb",
        offset: [0.000_000_3, 0.061_296_5, 0.0296],
        min: [-0.031_794, 0.0, -0.005_127_3],
        max: [0.032_963_2, 0.094_578_5, -0.000_117_7],
    },
    Panel {
        path: "environments/home/props/vodka_bottle_piece_back.glb",
        offset: [0.000_000_3, 0.076_346_5, -0.0296],
        min: [-0.032_963_9, 0.0, 0.000_117_7],
        max: [0.031_793_3, 0.094_578_5, 0.005_127_3],
    },
    Panel {
        path: "environments/home/props/vodka_bottle_piece_side.glb",
        offset: [0.036_917_3, 0.099_996_5, -0.015_329],
        min: [-0.011_424_4, 0.0, -0.011_221],
        max: [-0.000_172_7, 0.094_578_5, 0.038_097_5],
    },
];

const WINE: [Panel; 5] = [
    Panel {
        path: "environments/home/props/wine_bottle_piece_heel.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.039, 0.0, -0.039],
        max: [0.039, 0.055_227_4, 0.039],
    },
    Panel {
        path: "environments/home/props/wine_bottle_piece_neck.glb",
        offset: [0.0, 0.229, 0.0],
        min: [-0.019, 0.003_199_6, -0.019],
        max: [0.019, 0.081, 0.019],
    },
    Panel {
        path: "environments/home/props/wine_bottle_piece_front.glb",
        offset: [0.0, 0.068_709_1, 0.039],
        min: [-0.023_911_6, 0.0, -0.013_050_9],
        max: [0.026_867_9, 0.106_015_9, -0.000_498_1],
    },
    Panel {
        path: "environments/home/props/wine_bottle_piece_back.glb",
        offset: [0.0, 0.085_579_1, -0.039],
        min: [-0.026_867_9, 0.0, 0.000_723_3],
        max: [0.023_911_6, 0.106_015_9, 0.013_050_9],
    },
    Panel {
        path: "environments/home/props/wine_bottle_piece_side.glb",
        offset: [0.038_719_5, 0.112_089_1, -0.004_668_8],
        min: [-0.013_908_8, 0.0, -0.022_606_6],
        max: [-0.000_581_2, 0.106_015_9, 0.021_601_8],
    },
];

const GIN: [Panel; 5] = [
    Panel {
        path: "environments/home/props/gin_bottle_piece_heel.glb",
        offset: [0.0, 0.0, 0.0],
        min: [-0.045_509_4, 0.0, -0.037_317_7],
        max: [0.045_509_4, 0.053_227_4, 0.037_317_7],
    },
    Panel {
        path: "environments/home/props/gin_bottle_piece_neck.glb",
        offset: [0.0, 0.166, 0.0],
        min: [-0.019, 0.003_199_6, -0.019],
        max: [0.019, 0.065, 0.019],
    },
    Panel {
        path: "environments/home/props/gin_bottle_piece_front.glb",
        offset: [0.0, 0.050_747_8, 0.037_916_2],
        min: [-0.036_068_1, 0.0, -0.007_904_2],
        max: [0.038_281_9, 0.078_302_2, 0.000_197_9],
    },
    Panel {
        path: "environments/home/props/gin_bottle_piece_back.glb",
        offset: [0.0, 0.063_207_8, -0.038_092_4],
        min: [-0.038_458_9, 0.0, -0.000_164_8],
        max: [0.036_234_7, 0.078_302_2, 0.007_931_3],
    },
    Panel {
        path: "environments/home/props/gin_bottle_piece_side.glb",
        offset: [0.046_616_4, 0.082_787_8, -0.012_723_7],
        min: [-0.011_785_4, 0.0, -0.019_330_8],
        max: [-0.000_686_7, 0.078_302_2, 0.037_671_9],
    },
];
