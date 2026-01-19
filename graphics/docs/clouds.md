# Volumetric Cloud Pipeline

This document describes the Nubis-inspired volumetric cloud pipeline implemented in the renderer. It covers the data formats, pass IO, and tuning knobs exposed by `CloudSettings`.

## Data Formats

### WeatherMap2D
* **Format**: `RGBA8`.
* **Resolution**: `CloudNoiseSizes.weather_map_size` (default 256).
* **Channels**:
  * **R** – coverage `[0..1]`.
  * **G** – cloud type `[0..1]` (used to bias vertical density and detail response).
  * **B** – thickness `[0..1]`.
  * **A** – reserved (unused in the current pipeline).

### BaseNoise3D / DetailNoise3D
* **Format**: `R8`.
* **Encoding**: 3D noise packed into a 2D atlas (width = `size.x * size.z`, height = `size.y`).
* **Default sizes**:
  * Base noise: `128³`.
  * Detail noise: `32³`.

### BlueNoise2D
* **Format**: `RG8`.
* **Resolution**: `128²`.

### CloudShadowMap2D
* **Format**: storage buffer of `float` (transmittance).
* **Resolution**: `CloudShadowSettings.resolution` (default 256).

### Cloud Raymarch Outputs
* **CloudColor**: storage buffer of `vec4` (RGB + padding).
* **CloudTransmittance**: storage buffer of `float`.
* **CloudDepth**: storage buffer of `float` (ray distance).
* **Step heatmap**: storage buffer of `float` `[0..1]`.

## Passes

### Pass A – Cloud Shadow
* **Inputs**:
  * WeatherMap2D (`RGBA8`).
  * BaseNoise3D atlas (`R8`).
  * DetailNoise3D atlas (`R8`).
  * Cloud layer parameters (base/top altitude, density, wind, coverage).
  * Sun direction.
* **Output**:
  * `CloudShadowMap2D` transmittance buffer (`float`).
* **Notes**:
  * Shadow map is centered around the camera with a stable world-space extent.

### Pass B – Cloud Ray March (Low-Res)
* **Inputs**:
  * Camera matrices and position.
  * WeatherMap2D, BaseNoise3D, DetailNoise3D.
  * BlueNoise2D.
  * Sun direction + radiance.
  * Optional CloudShadowMap2D (falls back to light marching if disabled).
  * Wind + time.
* **Outputs (low-res)**:
  * `CloudColor` (HDR `vec4` buffer).
  * `CloudTransmittance` (`float`).
  * `CloudDepth` (`float` distance).
  * `Steps` heatmap (`float`).
* **Notes**:
  * Uses exponential transmittance integration per step.
  * Early-out when `transmittance < epsilon`.

### Pass C – Temporal Reprojection
* **Inputs**:
  * Current low-res `CloudColor`, `CloudTransmittance`, `CloudDepth`.
  * Previous history buffers.
  * Current + previous view-projection matrices.
* **Outputs**:
  * Updated history buffers for color, transmittance, depth, and weight.
* **Notes**:
  * Includes history clamping and depth-based disocclusion damping.

### Pass D – Upsample + Composite
* **Inputs**:
  * Low-res history buffers.
  * Full-res scene depth.
* **Output**:
  * Blended into the scene color via `Final = SceneColor * T + CloudColor`.
* **Notes**:
  * Depth-aware upsample prevents bleeding onto near geometry.
  * Debug views are selectable through `CloudDebugView`.

## Tuning Knobs (`CloudSettings`)

* **Altitude**: `base_altitude`, `top_altitude`.
* **Density**: `density_scale`.
* **Raymarch Steps**: `step_count`, `light_step_count`.
* **Phase**: `phase_g` (Henyey–Greenstein).
* **Wind**: `wind`, `wind_speed`.
* **Resolution**: `low_res_scale` (1/2 or 1/4).
* **Shadow**: `shadow.enabled`, `shadow.resolution`, `shadow.extent`, `shadow.strength`.
* **Temporal**: `temporal.blend_factor`, `temporal.clamp_strength`, `temporal.depth_sigma`.
* **Debug**: `debug_view` for weather map, shadow map, transmittance, step heatmap, temporal weight.
* **Budget**: `performance_budget_ms` (recorded in overlay, not enforced).

## Debug Views

`CloudDebugView` supports the following visualization modes:

1. **WeatherMap** – full-screen weather map.
2. **ShadowMap** – cloud shadow transmittance.
3. **Transmittance** – low-res cloud transmittance.
4. **StepHeatmap** – normalized step usage.
5. **TemporalWeight** – history weight/disocclusion visualization.

## Determinism

* Noise textures are generated deterministically from fixed seeds.
* Blue-noise jitter uses the frame index as a stable seed per frame.
* With fixed inputs (camera, sun, weather), the output is deterministic aside from temporal jitter.
