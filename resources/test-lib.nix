/**
  Utility functions
*/
{ lib }:

{
  /**
    A simple option
  */
  simpleOption =
    default: description:
    lib.mkOption {
      type = lib.types.str;
      inherit default description;
    };

  mergeAttrs = left: right: lib.recursiveUpdate left right;
}
