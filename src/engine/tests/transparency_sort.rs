//! Eight scalar entries protect original stable transparent ordering, including
//! equal distances and non-finite total ordering, without graphics or baking.
use super::{TransparentEntry, TransparentItem, sort_transparency};

#[test]
fn scratch_transparency_sort_matches_stable_distance_order_and_ties() {
    let distances = [1.0_f32, 9.0, 1.0, f32::NAN, 0.0, f32::INFINITY, 9.0, -0.0];
    let mut expected: Vec<_> = distances.into_iter().enumerate().collect();
    expected.sort_by(|left, right| right.1.total_cmp(&left.1));
    let mut actual: Vec<_> = distances
        .into_iter()
        .enumerate()
        .map(|(ordinal, distance)| TransparentEntry {
            item: TransparentItem::Prop(ordinal),
            ordinal,
            distance,
        })
        .collect();
    sort_transparency(&mut actual);
    assert_eq!(
        actual.iter().map(|entry| entry.ordinal).collect::<Vec<_>>(),
        expected.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        "reused scratch and nonallocating sort preserve the exact stable order"
    );
}
