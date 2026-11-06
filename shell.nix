{ pkgs ? import <nixpkgs> { } }:

let
  libnxt = pkgs.callPackage ./nix/libnxt.nix { };
in
pkgs.mkShell {
  buildInputs = [
    # Nightly Rust + rust-src, needed for the Tier-3 `armv4t-none-eabi`
    # target: it has no prebuilt std/core artifacts, so building requires
    # `-Z build-std=core` (configured in .cargo/config.toml), which in turn
    # requires the `rust-src` component on a nightly toolchain. There's no
    # `rustup target add armv4t-none-eabi` -- the target isn't shipped as
    # a prebuilt artifact at all; `rust-src` is what lets `build-std`
    # compile `core` from source for it.
    pkgs.rustup

    # arm-none-eabi-{ld,objcopy,objdump,size,gdb}. GNU `ld` is used as the
    # linker of record for armv4t-none-eabi (configured in
    # .cargo/config.toml) rather than rust-lld, since this target ships
    # with no default linker script and the GBA Rust ecosystem's
    # experience with the same CPU family favors GNU ld for this style of
    # hand-written linker script.
    pkgs.gcc-arm-embedded

    # fwflash: talks to the NXT's built-in SAM-BA USB bootloader to push a
    # raw firmware image. Not packaged in nixpkgs, built from source here.
    libnxt
    pkgs.libusb1

    pkgs.just
  ];

  shellHook = ''
    export CARGO_TARGET_ARMV4T_NONE_EABI_LINKER=arm-none-eabi-ld
    rustup toolchain install nightly --component rust-src --no-self-update >/dev/null 2>&1 || true
    rustup default nightly >/dev/null 2>&1 || true
  '';
}
