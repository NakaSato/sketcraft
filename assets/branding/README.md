# SketCraft branding

A slab with a block pulled up out of it (push/pull), drawn as six blue faces with gaps between them. The source of the mark is `assets/icons/app/macos/source/SketCraft.icon/Assets/03-solid-blue-cutout.svg`; `assets/icons/app/shared/source/app_icon.rs` draws the same faces for the desktop window and the exported PNG and ICO files.

- `mark.svg`: the transparent mark.
- `logo.svg`: the mark on a light rounded tile, for places that need an opaque badge.
- `icon-*.png`: transparent renders of the mark from 16 to 1024 pixels. The Linux release package uses `icon-256.png`.
- `favicon.ico`: the Windows release package icon, a copy of `assets/icons/app/windows/exports/sketcraft-executable.ico`.
- `ketchup.icns`: the macOS release package icon, a copy of `assets/icons/app/macos/exports/icon.icns`.
- `apple-touch-icon.png`: opaque 180 pixel tile.
- `icon-maskable-512.png`: opaque tile with extra padding for launcher masks.
- `site.webmanifest`: browser application metadata and launcher icons; paths support hosting in a subdirectory.

To change the mark, edit the SVG source and the `FACES` table in `app_icon.rs` together, then run `cargo xtask app-icons` for the Linux and Windows exports. Rebuild `assets/icons/app/macos/exports/` (Assets.car and icon.icns) with Xcode's `actool` from `SketCraft.icon`, and copy the results here.
