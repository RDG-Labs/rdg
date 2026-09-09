{ inputs, ... }:
{
  flake.overlays.default =
    final: _:
    let
      mkRdg = import ../toolchain.nix { inherit inputs; };
    in
    {
      rdg = mkRdg final;
    };
}
