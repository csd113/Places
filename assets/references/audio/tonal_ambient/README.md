# Original tonal ambience masters

MIT-licensed offline original synthesis, not a third-party recording library.
These files are not part of `assets/audio/` and must not enter its runtime catalog.

- Three 48 kHz mono PCM24 masters: `assets/references/audio/authoring/tonal.py:render_master(name)`.
- Exact favorite reference: `assets/references/audio/authoring/original.py:render("empty_room")`;
  44.1 kHz mono PCM16, unchanged original bytes, not the runtime conversion.
- `manifest.json` pins each source master and its format.
- Explicit 16 kHz runtime recipes/hashes live in `assets/audio/authoring.json`.

See [docs/assets/AUDIO.md](https://github.com/csd113/Places/blob/29e12d9aa52b6c54f1d5d2d46cf8a524cf5862f7/docs/assets/AUDIO.md) for format decisions, decoded memory and regeneration.
The pre-integration preparation/master archive is retained in Library as
`libfile_fde3_f-ba8e4c9f8328819199784d1e230aec4d_2e37728cdad08191acacf8a0401feb09`.
Neither numerical checks nor native mixer captures imply a perceptual listening
review. No supported auditory-review capability was available during preparation.
