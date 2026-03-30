# TODO

## Suggest install for unresolvable candidates

If a requested candidate version cannot be resolved (not installed), use the SDKMAN API to find the best matching uninstalled version. If one is found, print a suggested `sdk install` command to the user.

## Bootstrap initial .sdkvers file

Add a `--bootstrap` option (or similar) to `sdkvers`. If `.sdkvers` does not exist, this option creates one by querying the current shell's active versions for all installed SDK candidates.
