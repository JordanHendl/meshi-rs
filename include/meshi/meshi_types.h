#pragma once

#include <cstdint>

struct Handle {
    std::uint32_t index;
    std::uint32_t generation;
};

struct Vec2 {
    float x, y;
};

struct alignas(16) Vec3 {
    float x, y, z;
};

struct alignas(16) Vec4 {
    float x, y, z, w;
};

struct alignas(16) Quat {
    float x, y, z, w;
};

struct Mat4 {
    float m[4][4];
};

struct MeshiEngine;
struct RenderEngine;
struct PhysicsSimulation;
struct MeshObject;
struct DirectionalLight;
struct Material;
struct RigidBody;

struct MeshiEngineInfo {
    const char* application_name;
    const char* application_location;
    std::int32_t headless;
};

struct FFIMeshObjectInfo {
    const char* mesh;
    const char* material;
    Mat4 transform;
};

struct DirectionalLightInfo {
    Vec4 direction;
    Vec4 color;
    float intensity;
};

enum class EventType : std::uint32_t {
    Unknown = 0,
    Quit = 1,
    Pressed = 2,
    Released = 3,
    Joystick = 4,
    Motion2D = 5,
};

enum class EventSource : std::uint32_t {
    Unknown = 0,
    Key = 1,
    Mouse = 2,
    MouseButton = 3,
    Gamepad = 4,
    Window = 5,
};

enum class KeyCode : std::uint32_t {
    A = 0,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    Shift,
    Control,
    Alt,
    Meta,

    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    Escape,
    Enter,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,

    Minus,
    Equals,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Comma,
    Period,
    Slash,
    GraveAccent,

    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,

    CapsLock,
    NumLock,
    ScrollLock,

    PrintScreen,
    Pause,
    Menu,

    Undefined,
};

enum class MouseButton : std::uint32_t {
    Left,
    Right,
};

struct PressPayload {
    KeyCode key;
    EventType previous;
};

struct Motion2DPayload {
    Vec2 motion;
};

struct MouseButtonPayload {
    MouseButton button;
    Vec2 pos;
};

union Payload {
    PressPayload press;
    Motion2DPayload motion2d;
    MouseButtonPayload mouse_button;
};

struct Event {
    EventType event_type;
    EventSource source;
    Payload payload;
    std::uint32_t timestamp;
};

struct MaterialInfo {
    float dynamic_friction_m;
};

struct ForceApplyInfo {
    Vec3 amt;
};

enum class CollisionShapeType : std::uint32_t {
    Sphere = 0,
};

struct CollisionShape {
    CollisionShapeType shape_type;
    float radius;
};

struct RigidBodyInfo {
    Handle material;
    Vec3 initial_position;
    Vec3 initial_velocity;
    Quat initial_rotation;
    std::uint32_t has_gravity;
    CollisionShape collision_shape;
};

struct ActorStatus {
    Vec3 position;
    Quat rotation;
};

struct ContactInfo {
    Handle a;
    Handle b;
    Vec3 normal;
    float penetration;
};

using MeshObjectHandle = Handle;
using DirectionalLightHandle = Handle;
using MaterialHandle = Handle;
using RigidBodyHandle = Handle;

