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

/// A handful of ranges uniquely protect draw coalescing across materials,
/// overlapping actors and water/glass. No device or bake; execution is sub-ms.
#[test]
fn character_triangle_runs_do_not_cross_other_transparent_submissions() {
    use super::{CharacterDraw, coalesce_character_triangles};
    use crate::engine::render::wgpu::character_transparency::CharacterSpan;
    let character = |actor, submesh, start| {
        TransparentItem::Character(CharacterDraw {
            character: actor,
            span: Some(CharacterSpan {
                batch: None,
                submesh,
                first_index: start,
                index_count: 3,
            }),
        })
    };
    let mut items: Vec<_> = [
        character(0, 0, 0),
        character(0, 0, 3),
        TransparentItem::World(0),
        character(0, 0, 6),
        character(1, 0, 0),
        character(0, 1, 9),
        character(0, 1, 12),
    ]
    .into_iter()
    .enumerate()
    .map(|(ordinal, item)| TransparentEntry {
        item,
        ordinal,
        distance: 1.0,
    })
    .collect();
    coalesce_character_triangles(&mut items);
    assert_eq!(items.len(), 5, "only two same-state adjacent pairs merge");
    let spans: Vec<_> = items
        .iter()
        .filter_map(|entry| match entry.item {
            TransparentItem::Character(draw) => draw.span.map(|span| {
                (
                    draw.character,
                    span.submesh,
                    span.first_index,
                    span.index_count,
                )
            }),
            TransparentItem::World(_) | TransparentItem::Prop(_) | TransparentItem::Dynamic(_) => {
                None
            }
        })
        .collect();
    assert_eq!(
        spans,
        [(0, 0, 0, 6), (0, 0, 6, 3), (1, 0, 0, 3), (0, 1, 9, 6)],
        "materials, actors and intervening water/glass retain order"
    );
    // A compatible GPU stream stores the actor and emission per vertex. This
    // exception is essential: retaining actor boundaries recreated thousands
    // of tiny draws in the original seven-ghost native reproduction.
    let mut combined: Vec<_> = (0..2)
        .map(|actor| TransparentEntry {
            item: TransparentItem::Character(CharacterDraw {
                character: actor,
                span: Some(CharacterSpan {
                    batch: Some(0),
                    submesh: 0,
                    first_index: u32::try_from(actor).unwrap_or(u32::MAX).saturating_mul(3),
                    index_count: 3,
                }),
            }),
            ordinal: actor,
            distance: 1.0,
        })
        .collect();
    coalesce_character_triangles(&mut combined);
    assert_eq!(
        combined.len(),
        1,
        "adjacent compatible stream triangles merge across actors"
    );
    assert!(
        combined
            .first()
            .and_then(|entry| {
                if let TransparentItem::Character(draw) = entry.item {
                    draw.span
                } else {
                    None
                }
            })
            .is_some_and(|span| span.index_count == 6),
        "the combined range retains both triangles"
    );
}
