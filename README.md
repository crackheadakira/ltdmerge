# LTD Merge

ltdmerge is a tool to streamline the process of creating new items in the Mii editor.

# Usage

## Creating a mod

```sh
ltdmerge add --base /path/to/game/romfs --model /path/to/custom_model --icon /path/to/your_png --out /path/to/output_dir
```

> [!NOTE]
> Passing in --icon is fully optional, it will simply use an existing icon from the game instead.

## Merging mods

```sh
ltdmerge merge /path/to/mod1 /path/to/mod2 --out /path/to/output_dir
```

# License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
