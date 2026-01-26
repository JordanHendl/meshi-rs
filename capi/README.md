This is the C API for the engine. Scripting frameworks and native C/C++ consumers should load the plugin ABI via
`meshi_plugin_get_api` to obtain a stable function table instead of calling the raw symbols directly.
