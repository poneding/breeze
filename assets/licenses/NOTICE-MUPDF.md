# MuPDF dependency notice

Breeze uses `mupdf` and `mupdf-sys` version 0.8.0 from `mupdf-rs`, with the
`base14-fonts` and `system-fonts` features. `mupdf-sys` builds the MuPDF source
and its enabled third-party components. MuPDF's AGPL text is preserved in
`COPYING-MUPDF.txt`; the application license is at the package root as `LICENSE`.

The corresponding source for each Breeze release is attached to that release as
`Breeze-<version>-source.tar.gz`. It contains the exact Breeze source tree,
`Cargo.lock`, build/release scripts, and vendored locked Cargo source packages.
Each vendored crate carries its own license/notice files. This notice does not
replace those files or the source archive.
