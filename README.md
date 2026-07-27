Vhrd.Tech KiCad library
---

## Configuration

- Preferences - Manage Symbol Libraries - Folder down - KiCad (.kicad_sym)
    - Select all files from `symbols`
- Preferences - Manage Footprint Libraries - Folder down - KiCad (...)
    - Select all folders from `footprints`
- Preferences - Configure paths
    - Set `VHRD_3DMODEL_DIR` to `3dmodels` folder
    - Set `KICAD_USER_TEMPLATE_DIR` to `templates` folder if you want to use project templates

## KiKit

Install KiKit for panelization support: [Installation](https://yaqwsx.github.io/KiKit/latest/installation/intro)

Install just to create shortcuts for various commands like: `just panelize`:
[Just](https://github.com/casey/just#installation)

[Panelization Examples](https://yaqwsx.github.io/KiKit/latest/panelization/examples/)
[Panelization Docs](https://yaqwsx.github.io/KiKit/latest/panelization/cli/)

### Arch

`sudo pacman -S python-kikit`

