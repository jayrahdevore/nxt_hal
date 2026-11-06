# nixpkgs has no `libnxt` package (confirmed via `nix eval nixpkgs#libnxt`),
# so this builds it from source. It provides `fwflash`, which talks to the
# NXT's built-in SAM-BA USB bootloader to push a raw firmware image.
{
  lib,
  stdenv,
  fetchFromGitHub,
  meson,
  ninja,
  pkg-config,
  libusb1,
  python3,
  gcc-arm-embedded,
}:
stdenv.mkDerivation rec {
  pname = "libnxt";
  version = "0.5.2";

  src = fetchFromGitHub {
    owner = "schodet";
    repo = "libnxt";
    rev = "fbc8d357e4d4dfcc257d7939cbb4b2b0d93890fb"; # tag 0.5.2
    hash = "sha256-eleaociuykFghlyG8bmLWSak8N+6QWGnVHM13KuAO2o=";
  };

  nativeBuildInputs = [
    meson
    ninja
    pkg-config
    python3 # used by make_flash_header.py at build time
    gcc-arm-embedded # builds flash_write, a small blob libnxt uploads to the NXT itself
  ];

  buildInputs = [ libusb1 ];

  mesonFlags = [ "-Dbuild_doc=disabled" ]; # avoids depending on scdoc just for man pages

  meta = {
    description = "Low-level USB/SAM-BA utility library and fwflash tool for the LEGO Mindstorms NXT";
    homepage = "https://github.com/schodet/libnxt";
    license = lib.licenses.gpl2Plus;
    platforms = lib.platforms.linux;
    mainProgram = "fwflash";
  };
}
