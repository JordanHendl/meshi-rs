# Volumetric Cloud Pipeline

This document describes the Nubis-inspired volumetric cloud pipeline implemented in the renderer. It covers the data formats, pass IO, tuning knobs exposed by `CloudSettings`, and how the implementation maps onto the "Nubis" style of cloud rendering (including what is present or missing).

## High-Level Overview

The cloud system is a two-layer, view-ray-marched volume with optional shadow prepass and temporal reuse:

1. **Weather-driven density field** – a 2D weather map provides coverage/type/thickness. This drives a layered density function that combines base and detail noise.
2. **Low-res raymarch** – the camera frustum is intersected against two cloud slabs (`layer_a` + `layer_b`). Density is sampled along view rays and integrated with Beer–Lambert extinction, then composited in depth order.
3. **Lighting and shadows** – lighting uses a Henyey–Greenstein phase function with either a directional sun or scene lights. A separate shadow pass generates a top-down transmittance map to accelerate sun lighting.
4. **Temporal reprojection** – history buffers (color/transmittance/depth/weight) are reprojected and clamped to stabilize noise/jitter.
5. **Depth-aware upsample + composite** – low-res clouds are upsampled against scene depth, then blended into the scene color.

The pipeline follows the Nubis pattern of a lightweight weather map + 3D noise stack, but it is intentionally simplified to keep the runtime cost small and predictable.

## Data Formats

### WeatherMap2D
* **Format**: `RGBA8`.
* **Resolution**: `CloudNoiseSizes.weather_map_size` (default 256).
* **Channels**:
  * **R** – coverage `[0..1]`.
  * **G** – cloud type `[0..1]` (used to bias vertical density and detail response).
  * **B** – thickness `[0..1]`.
  * **A** – layer B coverage `[0..1]` (layer B shares G/B for type/thickness by default).

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
* **Resolution**: `CloudShadowSettings.resolution` (default 256) per cascade, with cascade-specific overrides via `ShadowCascadeSettings.cascade_resolutions`.
* **Layout**: cascades are packed linearly into the buffer using per-cascade offsets/resolutions computed on the CPU each frame.

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
  * Cloud layer parameters (base/top altitude, density, noise scale, wind, coverage).
  * Sun direction.
* **Output**:
  * `CloudShadowMap2D` transmittance buffer (`float`).
* **Notes**:
  * Shadow map is centered around the camera with a stable world-space extent.
  * Cascades use the same split computation as the opaque shadow system (log/linear blend via `split_lambda`).
  * The shader marches a fixed number of steps (12) from the top of the cloud slab to the base to estimate transmittance along the sun direction.

### Pass B – Cloud Ray March (Low-Res)
* **Inputs**:
  * Camera matrices and position.
  * WeatherMap2D, BaseNoise3D, DetailNoise3D.
  * BlueNoise2D.
  * Sun direction + radiance.
  * Optional CloudShadowMap2D (falls back to light marching if disabled).
  * Per-layer wind + time.
* **Outputs (low-res)**:
  * `CloudColor` (HDR `vec4` buffer).
  * `CloudTransmittance` (`float`).
  * `CloudDepth` (`float` distance).
  * `Steps` heatmap (`float`).
* **Notes**:
  * Uses exponential transmittance integration per step.
  * Early-out when `transmittance < epsilon`.
  * Blue-noise jitter offsets the starting point along the ray to reduce banding.
  * The raymarch operates on fixed-height slabs; there is no horizon intersection with a curved atmosphere.

### Pass C – Temporal Reprojection
* **Inputs**:
  * Current low-res `CloudColor`, `CloudTransmittance`, `CloudDepth`.
  * Previous history buffers.
  * Current + previous view-projection matrices.
* **Outputs**:
  * Updated history buffers for color, transmittance, depth, and weight.
* **Notes**:
  * Includes history clamping and depth-based disocclusion damping.
  * Reprojection uses the previous view-projection matrix and reconstructs world position from the cloud depth buffer.

### Pass D – Upsample + Composite
* **Inputs**:
  * Low-res history buffers.
  * Full-res scene depth.
* **Output**:
  * Blended into the scene color via `Final = SceneColor * T + CloudColor`.
* **Notes**:
  * Depth-aware upsample prevents bleeding onto near geometry.
  * Debug views are selectable through `CloudDebugView`.
  * The composite shader produces premultiplied cloud color with alpha = `1 - transmittance`.

## Tuning Knobs (`CloudSettings`)

* **Layer A**: `layer_a.base_altitude`, `layer_a.top_altitude`, `layer_a.density_scale`, `layer_a.noise_scale`, `layer_a.wind`, `layer_a.wind_speed`.
* **Layer B**: `layer_b.base_altitude`, `layer_b.top_altitude`, `layer_b.density_scale`, `layer_b.noise_scale`, `layer_b.wind`, `layer_b.wind_speed`.
* **Raymarch Steps**: `step_count`, `light_step_count`.
* **Phase**: `phase_g` (Henyey–Greenstein).
* **Multi-scatter**: `multi_scatter_strength`, `multi_scatter_respects_shadow`.
* **Resolution**: `low_res_scale` (1/2 or 1/4).
* **Atmosphere**: `atmosphere_view_strength`, `atmosphere_view_extinction`, `atmosphere_light_transmittance`, `atmosphere_haze_strength`, `atmosphere_haze_color`.
* **Shadow**: `shadow.enabled`, `shadow.resolution`, `shadow.extent`, `shadow.strength`, `shadow.cascades.cascade_count`, `shadow.cascades.split_lambda`, `shadow.cascades.cascade_extents`, `shadow.cascades.cascade_resolutions`, `shadow.cascades.cascade_strengths`.
* **Temporal**: `temporal.blend_factor`, `temporal.clamp_strength`, `temporal.depth_sigma`.
* **Debug**: `debug_view` for weather map, shadow map, per-cascade shadow buffers, transmittance, step heatmap, temporal weight, stats, and single vs. multi scatter (plus opaque shadow cascades).
* **Budget**: `performance_budget_ms` (recorded in stats overlay, not enforced).

## Debug Views

`CloudDebugView` supports the following visualization modes:

1. **WeatherMap** – full-screen weather map.
2. **ShadowMap** – cloud shadow transmittance (cascade 0).
3. **Transmittance** – low-res cloud transmittance.
4. **StepHeatmap** – normalized step usage.
5. **TemporalWeight** – history weight/disocclusion visualization.
6. **Stats** – enable the cloud performance timing overlay.
7. **LayerA** – isolate layer A.
8. **LayerB** – isolate layer B.
9. **SingleScatter** – disable multi-scatter gain.
10. **MultiScatter** – visualize the multi-scatter boost alone.
11. **Cloud Shadow Cascade 0-3** – view packed cloud shadow cascades individually.
12. **Opaque Shadow Cascade 0-3** – view the deferred opaque shadow atlas per cascade.

## Determinism

* Noise textures are generated deterministically from fixed seeds.
* Blue-noise jitter uses the frame index as a stable seed per frame.
* With fixed inputs (camera, sun, weather), the output is deterministic aside from temporal jitter.

## What Happens Inside the Raymarch (Detailed)

The raymarch shader performs the following steps per pixel:

1. **Frustum ray setup** – the pixel's NDC is unprojected to a world-space ray. The ray is intersected against each cloud slab defined by `layer_a`/`layer_b`. The first sample is jittered with blue noise.  
2. **Weather sampling** – the weather map is sampled at `world.xz` with wind/time offsets to drive large-scale variation:
   * **Coverage**: `weather.r` raised to `coverage_power` controls overall occupancy.
   * **Type**: `weather.g` biases the vertical response and detail influence.
   * **Thickness**: `weather.b` attenuates density as the ray approaches the top of the layer.
3. **Density shaping** – a base noise sample defines macro structure and a detail noise sample modulates it:
   * Base noise is sampled in 3D (atlas) and optionally displaced with curl noise (`curl_strength`).
   * Detail noise is sampled at a higher frequency.
   * Density = `(base_noise * coverage - (1 - thickness) * (1 - height_frac))`, then modulated by detail and type.
4. **Single scattering + extinction** – for each step:
   * Extinction uses Beer–Lambert: `step_trans = exp(-sigma_t * step_size)`.
   * A Henyey–Greenstein phase function (`phase_g`) modulates anisotropy.
   * Lighting uses either the directional sun (if no explicit scene lights) or the engine's light list.
   * A multi-scattering gain term (`multi_scatter_strength`) boosts energy based on local extinction, optionally gated by shadowing.
   * Light contributions are attenuated by a coarse atmospheric transmittance factor.
5. **Shadowing** – sun lighting can sample the shadow buffer (if enabled), otherwise a mini light march is performed along the light direction.
6. **Accumulation** – scattered radiance is accumulated with the current transmittance and the transmittance is updated for the next step.
7. **Depth output** – the shader stores a weighted average depth along the ray so reprojection can reconstruct world positions.

These steps follow the typical Nubis-style split between large-scale weather control and high-frequency noise detail, while adding a cheap multi-scatter boost and coarse atmospheric coupling.

## Temporal Reprojection and Stability (Detailed)

The temporal pass reconstructs world-space positions for the current low-res cloud pixel using the stored cloud depth. That world position is projected into the previous frame to fetch a history sample. The pass then:

* **Clamps** the history color to the current 3×3 neighborhood min/max (to reduce ghosting).
* **Depth-weights** the history using an exponential falloff (`depth_sigma`) so disocclusions blend toward current data.
* **Blends** color, transmittance, depth, and history weight using `blend_factor`.

This scheme keeps low-res jittered marching stable in motion, while still respecting sudden density changes from weather, wind, or camera movement.

## Composite and Debug Output (Detailed)

The composite shader:

* Bilinearly upsamples the low-res cloud buffers.
* Compares cloud depth against the scene depth; if the cloud depth is behind geometry beyond `depth_sigma`, the cloud contribution is suppressed.
* Outputs premultiplied cloud color with alpha equal to `1 - transmittance`.
* Applies a view-space aerial perspective term that fades clouds toward a configurable haze color.
* Supports debug views for weather, shadow, transmittance, step usage, temporal weight, and single vs. multi-scatter comparison.

## Relation to the Nubis-Style Cloud Pipeline

The implementation mirrors Nubis-inspired techniques in several key ways:

* **Weather map driving macrostructure** – the 2D weather map provides large-scale control over coverage and cloud type, similar to Nubis' low-frequency control texture.
* **Two-tier noise stack** – base 3D noise defines the overall mass while a higher-frequency detail noise adds erosion-like breakup.
* **Height-based shaping** – density is reduced near the cloud top and modulated by thickness, echoing Nubis' height gradient and vertical profile shaping.
* **Phase-based single scattering** – a Henyey–Greenstein phase function drives directional light response, matching the typical Nubis single-scatter approximation.
* **Temporal reuse + low-res march** – Nubis-style implementations rely on low-res raymarching with temporal accumulation to keep costs low; this renderer does the same.

## What's Present vs. Missing (Compared to a Full Nubis Stack)

**Present (implemented in this renderer):**

* Two cloud slabs with configurable base/top altitude (layer A + layer B).
* Weather-driven coverage/type/thickness.
* Base + detail 3D noise, plus optional curl noise distortion.
* Directional sun lighting with either shadow map sampling or short light marches.
* Single-scatter lighting with an optional multi-scatter gain term.
* Atmospheric transmittance/haze coupling in lighting and composite.
* Temporal reprojection with clamping and depth-based rejection.
* Debug views that expose intermediate buffers.

**Missing / Simplified compared to a full Nubis pipeline:**

* **No curved atmosphere intersection** – slabs are flat, so horizon curvature is not modeled.
* **No explicit erosion/ambient occlusion maps** – Nubis frequently uses erosion or curl noise textures to carve edges; here, erosion is approximated via detail noise.
* **No energy-conserving multiple scattering** – the multi-scatter term is a heuristic gain rather than a physical integration.
* **Simplified atmospheric coupling** – uses coarse transmittance/haze parameters instead of full LUT-based sky coupling.
* **Simplified shadowing** – the shadow pass uses a fixed step count and does not incorporate multi-scattering or penumbra widening.
* **No weather evolution model** – weather changes are driven only by wind/time offsets, not by a simulation or evolving 3D weather volume.

These tradeoffs keep the implementation fast and deterministic, while still achieving the recognizable Nubis-style look: large-scale coverage control with layered noise detail and temporal stability.
