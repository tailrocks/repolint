# Repository instructions

Keep M1 narrow: repolint owns repository-map generation and its drift gate.
Mechanical file, TOML, grep, and generated-freshness checks belong to alint;
GitHub Actions security checks belong to zizmor.

Do not hand-edit the marked README map. Change `[map.dirs]` or `[map.files]`,
then run `repolint map --write`.
