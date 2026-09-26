# Core / generic assets

Shared content that belongs to no environment theme: the domestic and utility
props used across office, pool and user-created levels.

`props/models/*.glb` holds the fifteen generic props (couch, armchair, table,
bookshelf, bed, stove, sink, fridge, washing machine, crate, cardboard box,
plant, rug, lamp, TV). Their catalog entries carry no `theme`, which is the
architecture's way of saying "use me anywhere".

`core` is also the asset **class** for engine-level shared resources (see
`asset_class: core` in the catalog), so a future shared shader, icon or UI
texture has an obvious home here without becoming an environment theme.

This directory is not "miscellaneous": generic props still carry full metadata,
budgets and validation exactly like themed ones.
