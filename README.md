# LTD Merge

ltdmerge is a tool to streamline the process of creating new items in the Mii editor.

# Usage

## Creating a mod

```sh
ltdmerge add --base /path/to/game/romfs --out /path/to/output_dir manifest.json
```

The manifest contains an `assets` array. Model categories use `model_unit.fmdb`
as the local BFRES source; Eye uses `texture_name` as the local PNG source.
An optional `editor_icon_name` supplies a PNG, otherwise a vanilla icon is reused.

```json
{
    "assets": [{ "category": "Eye", "texture_name": "/path/to/eye.png" }]
}
```

## Merging mods

```sh
ltdmerge merge /path/to/mod1 /path/to/mod2 --out /path/to/output_dir
```

# License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
