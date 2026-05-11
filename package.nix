{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsHook4,
  cargo-tauri,
  librsvg,
  webkitgtk_4_1,
  glib-networking,
  makeBinaryWrapper,
}:

rustPlatform.buildRustPackage {
  pname = "marimo-tauri";
  version = "0.1.0";

  src = lib.cleanSource ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  # The Tauri config points frontendDist to ./dist (a checked-in static
  # index.html), so no JS build step is needed.
  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
    cargo-tauri.hook
    makeBinaryWrapper
  ];

  buildInputs = [
    librsvg
    webkitgtk_4_1
    glib-networking
  ];
  preFixup = ''
    gappsWrapperArgs+=(
      --set WEBKIT_DISABLE_DMABUF_RENDERER 1
      --set WEBKIT_DISABLE_COMPOSITING_MODE 1
      --set WEBKIT_FORCE_SANDBOX 0
    )
  '';
  # cargo-tauri.hook drives `cargo tauri build`; it picks up tauri.conf.json
  # from the source root.

  # Don't run tests — there are none and the bundle step is the build.
  doCheck = false;

  meta = {
    description = "Tauri wrapper around a marimo notebook server";
    mainProgram = "app";
    platforms = lib.platforms.linux;
  };
}
