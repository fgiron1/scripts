pub mod recon;
pub mod scan;
pub mod exploit;

// Macro to help with plugin registration
#[macro_export]
macro_rules! register_plugin {
    ($plugin:ty) => {
        pub fn create_plugin() -> Box<dyn Plugin> {
            Box::new(<$plugin>::default())
        }
    };
}
