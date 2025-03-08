#[derive(Debug, Clone)]  
pub enum ControllerEvent {
    ButtonPress { button: Button, pressed: bool },
    AxisMove { axis: Axis, value: f32 },
    TouchpadEvent { x: i32, y: i32 },
}