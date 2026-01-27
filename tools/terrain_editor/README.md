# Terrain Editor

Early scaffolding for a GUI terrain editor that will use meshi-rs graphics + noren's dbgen to
preview and author terrain RDB data (procedural generation and manual edits).

## Goals
- Launch a windowed GUI based on `meshi-graphics`.
- Render terrain chunks from noren RDB terrain artifacts.
- Integrate noren dbgen for procedural generation and manual brush edits.

## Current Status
- Window + render loop established.
- Status HUD shows editor mode (procedural/manual).
- Terrain dbgen adapter uses noren terrain build pipeline to generate/brush RDB artifacts.

## Controls
- `Tab`: toggle between Procedural and Manual modes.

## Next Steps
- Add brushes/tools for manual terrain edits.
- Load/save terrain databases.
