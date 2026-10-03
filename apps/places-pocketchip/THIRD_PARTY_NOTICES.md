# PocketCHIP Places dependency notices

Audit date: **2026-10-02**, package **0.11.2**, ARMv7 GNU/Linux.
Project code and artwork use [MIT](LICENSE). External dependencies retain
upstream terms. Distribute this file and [verbatim texts](THIRD_PARTY_LICENSES.txt)
with the package; the project MIT grant does not relicense those dependencies.

The table records the locked Cargo dependency set resolved for ARMv7, including
build-time crates. It chooses MIT where offered as an alternative, retaining
both terms for AND expressions. Text IDs are the first 16 hex digits of each
upstream file's SHA-256. The existing collected file also contains example,
test and other-target notices; their presence does not establish linkage.

SDL2 is supplied dynamically by the operating system through `use-pkgconfig`;
the bundled-SDL feature is disabled. The Rust wrapper and SDL2-sys 0.38.0
share upstream revision `52de59121c087d43b555f6d9e1aaae35a0cb0ed6`.
The repository-root wrapper MIT text and SDL source Zlib text are both retained.
System graphics libraries and operating-system packages are not distributed in
this app payload. The Rust standard library also retains its upstream terms;
the Rust toolchain copyright record is separate from this crate inventory.

| Crate | Locked version | Declared terms | Selected terms | Text IDs |
| --- | --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| bitflags | 1.3.2 | MIT/Apache-2.0 | MIT | 6485b8ed310d3f03 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | MIT | 6485b8ed310d3f03 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | MIT | 65f94e99ddaf4f5d |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | MIT | 378f5840b258e277 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | MIT | 61d383b05b87d78f |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | MIT | 7365cc8878a1d7ce |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | MIT | c77a4cf9da729987 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | MIT | 025436edff4cfcdd |
| glam | 0.29.3 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib | MIT | cdbd06e25b8c9c5d |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | MIT | ff8f68cb076caf8c |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | MIT | ecc269ef87fd38a1 |
| itoa | 1.0.18 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 | MIT | 0621878e61f0d0fd |
| libc | 0.2.189 | MIT OR Apache-2.0 | MIT | 123a331b5dbf04c3 |
| log | 0.4.34 | MIT OR Apache-2.0 | MIT | 6485b8ed310d3f03 |
| memchr | 2.8.3 | Unlicense OR MIT | MIT | 0f96a83840e146e4 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | MIT | 799e9ca9d179295e |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | MIT | 799e9ca9d179295e |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | MIT | 378f5840b258e277 |
| png | 0.18.1 | MIT OR Apache-2.0 | MIT | eaf40297c75da471 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| quote | 1.0.47 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| sdl2 | 0.38.0 | MIT | MIT | df7af208e28b219a |
| sdl2-sys | 0.38.0 | MIT AND Zlib | MIT AND Zlib | df7af208e28b219a, 9928507f684c1965 |
| serde | 1.0.229 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| simd-adler32 | 0.3.10 | MIT | MIT | 42a35170233e83e1 |
| syn | 3.0.6 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| typed-path | 0.12.3 | MIT OR Apache-2.0 | MIT | 23f18e03dc49df91 |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | MIT AND Unicode-3.0 | 23f18e03dc49df91, f7db81051789b729 |
| version-compare | 0.1.1 | MIT | MIT | cedfcc7ace1639ad |
| zip | 8.6.0 | MIT | MIT | 58545fed1565e42d |
| zlib-rs | 0.6.8 | Zlib | Zlib | e72111c52b7d96eb |
| zmij | 1.0.23 | MIT | MIT | 23f18e03dc49df91 |
| zopfli | 0.8.3 | Apache-2.0 | Apache-2.0 | 018b1cb87efdf7a0 |
