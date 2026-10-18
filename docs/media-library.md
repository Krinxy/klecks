# Media library

Your rooms and your furniture are folders. klecks reads them every time you open the Rooms or Catalog
screen, so adding a room or a piece of furniture means creating a folder (Settings > Folders > Open).

```
media/
  rooms/
    livingroom/            all videos of the living room
      IMG_7694.MP4
      walk.MOV
    kitchen/
  assets/
    sofa_grey/             one piece of furniture
      front.jpg            photos (jpg, png, webp)
      side.jpg
      asset.yaml           optional, see below
    round_table/
```

Where `media/` is: the `media_dir` in Settings > Folders, else a `media/` folder next to the program
or its repository, else `%LOCALAPPDATA%\klecks\media` (Linux: `~/.local/share/klecks/media`).

## Rooms

- One folder per room under `rooms/`; the folder name is the room's name (`living_room` shows as
  "Living room").
- Put **every video of that room** in it. They are reconstructed together, so several films of the
  same room become one scene.
- Supported: mp4, mov, mkv, webm, avi (any letter case); portrait and landscape, SDR and HDR may be mixed.
- A **walk through the room** helps most: a long film is sampled every two thirds of a second and
  reconstructed in overlapping parts, so minutes of film work (the longer, the longer it takes).
- **Reconstruct** scans the room; **Scan again** after adding videos. Finished scans are stored in the
  scans folder as `<room>-<time>` and listed with the room.

## Furniture (catalog)

A folder with just photos already works. Add an `asset.yaml` for the details:

```yaml
name: Corner sofa          # shown in the catalog; default: the folder name
category: sofa
width_cm: 280              # the real size; all three are needed to place it later
depth_cm: 160
height_cm: 85
notes: Grey fabric, bought 2023
source_url: https://example.com/the-sofa
```

Everything is optional. A missing size shows as "Size missing" in the catalog.

## Settings

The Settings screen saves what you change in `%APPDATA%\klecks\*.yaml` (Linux: `~/.config/klecks`) and
applies it right away. Those files only contain what you changed; deleting one brings the defaults
back. The Settings screen rewrites them, so comments you added by hand are not kept.
