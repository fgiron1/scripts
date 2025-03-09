use windows::core::{Result, ComInterface, GUID};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Devices::HumanInterfaceDevice::{
    IDirectInput8A, DirectInput8Create, IDirectInputDevice8A, DIDEVICEINSTANCEA
};
use std::ffi::c_void;
use std::collections::HashMap;
use crate::controller::{ControllerType, Controller, DirectInputController};
use crate::controller::types::{DriverConfig, AxisConfig, Input};


pub struct DriverSetup {
    pub direct_input: IDirectInput8A,
    pub device_guids: HashMap<String, GUID>, // Almacena los GUIDs de los dispositivos
    pub candidate_guids: Vec<GUID>,

}

impl DriverSetup {
    pub fn new(hinstance: HMODULE, user_candidate_guids: Option<Vec<GUID>>) -> Result<Self> {
        let mut direct_input = None;
        unsafe {
            DirectInput8Create(
                hinstance,
                0x0800,
                &IDirectInput8A::IID as *const GUID,
                &mut direct_input as *mut _ as *mut _,
                None
            )?;
        }

        // Inicializa la lista de GUIDs candidatos con valores predeterminados
        let mut candidate_guids = vec![
            GUID { data1: 0x054C, data2: 0x09CC, data3: 0, data4: [0; 8] }, // GUID del controlador inalámbrico
            // Puedes agregar más GUIDs candidatos aquí (clones de controladores oficiales)
        ];

        // Agrega los GUIDs proporcionados por el usuario si están presentes
        if let Some(user_guids) = user_candidate_guids {
            candidate_guids.extend(user_guids);
        }

        Ok(Self { 
            direct_input: direct_input.unwrap(),
            device_guids: HashMap::new(), // Inicializa el HashMap
            candidate_guids, // Inicializa la lista de GUIDs candidatos
        })
    }

    pub fn configure_directinput(&self) -> Result<DriverConfig> {
        let mut config = DriverConfig {
            axes: HashMap::new(),
        };
        
        config.axes.insert(Input::TrackpadX, AxisConfig { range: -1.0..1.0 });
        config.axes.insert(Input::TrackpadY, AxisConfig { range: -1.0..1.0 });
        
        Ok(config)
    }

    pub fn create_device(&self, guid: &GUID) -> Result<IDirectInputDevice8A> {
        let mut device: Option<IDirectInputDevice8A> = None;
        unsafe {
            self.direct_input.CreateDevice(
                guid,
                &mut device as *mut _ as *mut _,
                None
            )?;
        }
        Ok(device.unwrap())
    }

    pub fn enumerate_devices(&mut self) -> Result<()> {
        unsafe extern "system" fn enum_callback(
            instance: *mut DIDEVICEINSTANCEA,
            context: *mut c_void,
        ) -> windows::Win32::Foundation::BOOL {
            let instance = &*instance;
            let setup = &mut *(context as *mut DriverSetup); // Accede a DriverSetup
    
            // Convierte todos los campos a su representación hexadecimal en una sola línea
            let guid_string = format!(
                "{:X}-{:X}-{:X}-{}",
                instance.guidProduct.data1,
                instance.guidProduct.data2,
                instance.guidProduct.data3,
                instance.guidProduct.data4.iter()
                    .map(|byte| format!("{:02X}", byte)) // Convierte cada byte a hexadecimal
                    .collect::<Vec<String>>() // Recoge los resultados en un vector
                    .join("") // Une los elementos en una sola cadena
            );
    
            // Almacena el GUID en el HashMap
            setup.device_guids.insert(guid_string, instance.guidProduct);
    
            return windows::Win32::Foundation::BOOL(1); // Continúa enumerando, usando `BOOL(1)`
        }
    
        unsafe {
            let context = self as *mut _ as *mut c_void; // Crea un puntero mutable antes de la llamada
            self.direct_input.EnumDevices(
                0, // Puedes usar DIEDFL_ALLDEVICES para enumerar todos los dispositivos
                Some(enum_callback),
                context, // Pasa `self` como contexto
                0,
            )?;
        }
        Ok(())
    }

    pub fn filter_device_guids(&self) -> HashMap<String, GUID> {
        let mut filtered_guids = HashMap::new();

        for (guid_string, guid) in &self.device_guids {
            if self.candidate_guids.iter().any(|candidate| *candidate == *guid) {
                filtered_guids.insert(guid_string.clone(), *guid);
            }
        }

        filtered_guids
    }
}

#[cfg(target_os = "windows")]
pub fn create_controller(user_candidate_guids: Option<Vec<GUID>>) -> Result<(Box<dyn Controller>, ControllerType)> {
    let hinstance = unsafe { GetModuleHandleW(None)? }; // Directly use imported function
    let mut setup = DriverSetup::new(hinstance, user_candidate_guids)?; // Pasa la lista de GUIDs candidatos

    // Aquí enumeramos los dispositivos
    setup.enumerate_devices()?; // Esto llenará device_guids con los dispositivos encontrados

    // Filtra los device_guids según los candidate_guids
    let filtered_guids = setup.filter_device_guids();

    // Ahora, intenta crear un dispositivo usando el primer GUID encontrado
    if let Some((_, guid)) = filtered_guids.iter().next() { // Obtén el primer GUID encontrado
        let device = setup.create_device(guid)?; // Crea el dispositivo usando el GUID
        let dinput = DirectInputController::new(device)?; // Crea el controlador
        return Ok((Box::new(dinput), ControllerType::DirectInput));
    }

    Err(windows::core::Error::from_win32()) // Manejo de error si no se encuentra un dispositivo
}

