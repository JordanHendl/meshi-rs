// plugin_loader.hpp
#pragma once

#include <cstddef>

#if defined(_WIN32)
  #ifndef NOMINMAX
    #define NOMINMAX
  #endif
  #include <windows.h>
#else
  #include <dlfcn.h>
#endif

namespace meshi::detail {
// Loads a dynamic library (.dll/.so/.dylib) and returns an opaque handle.
// Returns nullptr on failure.
inline void* loader_function(const char* library_path) {
    if (!library_path || library_path[0] == '\0') {
        return nullptr;
    }

#if defined(_WIN32)
    // Prefer ANSI version since input is const char*. If you need UTF-8 paths,
    // convert to UTF-16 and call LoadLibraryW instead.
    HMODULE mod = ::LoadLibraryA(library_path);
    return reinterpret_cast<void*>(mod);
#else
    // RTLD_NOW: resolve symbols immediately (fail early).
    // RTLD_LOCAL: do not expose symbols globally (safer for plugins).
    void* handle = ::dlopen(library_path, RTLD_NOW | RTLD_LOCAL);
    return handle;
#endif
}

// Optional helpers you almost always need:

inline void* get_plugin_symbol(void* plugin_handle, const char* symbol_name) {
    if (!plugin_handle || !symbol_name || symbol_name[0] == '\0') {
        return nullptr;
    }

#if defined(_WIN32)
    FARPROC p = ::GetProcAddress(reinterpret_cast<HMODULE>(plugin_handle), symbol_name);
    return reinterpret_cast<void*>(p);
#else
    // Clear any previous error
    (void)::dlerror();
    void* p = ::dlsym(plugin_handle, symbol_name);
    // If you care: const char* err = ::dlerror();
    return p;
#endif
}

inline void unload_plugin(void* plugin_handle) {
    if (!plugin_handle) return;

#if defined(_WIN32)
    ::FreeLibrary(reinterpret_cast<HMODULE>(plugin_handle));
#else
    ::dlclose(plugin_handle);
#endif
}
}
