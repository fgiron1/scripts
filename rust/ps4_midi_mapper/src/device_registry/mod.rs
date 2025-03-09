pub mod controller;
pub mod metadata;
pub mod registry;

pub use self::{
    controller::{Controller, ControllerEvent, Axis, Button},
    metadata::DeviceMetadata,
    registry::{DeviceRegistry, InputDevice},
};