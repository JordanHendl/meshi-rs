# Koji double-free investigation

The `directional_light_render` test crashes with `free(): double free detected in tcache 2` when using the canvas backend.

This occurs after a static mesh is registered with Koji's renderer and the renderer is dropped. Koji's `Renderer::with_canvas_headless` clones the supplied `Canvas` into the render graph. Both the clone and the renderer attempt to free the underlying render target attachments on drop, leading to a double free within Koji's resource management.

A temporary workaround is to retain the original `Canvas` in a `Box` instead of cloning it, ensuring it isn't dropped independently of the renderer. However, Koji still clones internally, so the underlying lifetime problem remains. Further work will be required upstream in Koji to resolve the resource management.
