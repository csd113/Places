# Directional entity pixel test root correction

The original native Metal assertion compared linear irradiance with sRGB display bytes. The committed fallback PNG contributes UV(0,0) colour `(253,253,255)`, so the reference now decodes that texel, applies the unchanged directional light and default fog, and uses one sRGB encoding. The existing direct capture helper and renderer are unchanged.

Original and final facing pixels are `(127,127,128,255)`; reversed pixels are `(88,88,89,255)`. Their PNG hashes are identical across the failing and passing runs. Every channel keeps the strict `<2` code-value tolerance and `>20` reversed-direction contrast, and must reject the incorrect default +Z normal reference. Both alpha values must remain255.

The final explicitly allocated Metal command passed one test with2194filtered (`cargo test --locked --lib --all-features directional_entity_irradiance_reaches_real_rendered_pixels -- --ignored --test-threads=1 --nocapture`), exit0,0.36s test runtime. Original failures, the sandbox adapter limitation, the first corrected module-path compile failure, and numeric-reference pass remain preserved in separate directories.

Only the cfg(test) file `src/render/wgpu/entity_lighting_tests.rs` differs from source freezev10. Normal compiler SHA256 remains `f3f27db928af9927219f3440a95acbbcfb2f53bdecb321af207613ddce91527f`; normal player remains `f5d3a2fe572e4abc69237c6df14e527060b839901184c65b6b69830ad8628081`. These existing binaries were hashed without a release rebuild. No production renderer or asset change occurred.

[Machine-readable evidence](directional-pixel-root-v1.json) includes exact run receipts, image hashes, references and source integrity. Primary owns the remaining strict aggregate gate/diagnostic queue. All source and focused Cargo/GPU custody is released; no allocated job remains.
