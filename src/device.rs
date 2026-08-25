use serialport::{SerialPortType, UsbPortInfo};

/// デバイスのハードウェア固有番号を使用して、識別番号を作成する
fn make_device_id(port_info: &PortInfo) -> String {
    if let Some(serial_number) = &port_info.serial_number {
        format!(
            "{:04X}-{:04X}-{}",
            port_info.vid, port_info.pid, serial_number
        )
    } else {
        format!("{:04X}-{:04X}", port_info.vid, port_info.pid)
    }
}

/// USBポートの情報
#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// コンピューターに接続されて利用可能なシリアルポートデバイスの情報
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// ポート名
    pub port_name: String,
    /// 取得できたポート情報
    pub usb_port_info: PortInfo,
    /// ポート情報から生成されたデバイスID
    pub device_id: String,
}

impl From<UsbPortInfo> for PortInfo {
    fn from(value: UsbPortInfo) -> Self {
        Self {
            vid: value.vid,
            pid: value.pid,
            serial_number: value.serial_number,
            manufacturer: value.manufacturer,
            product: value.product,
        }
    }
}

/// 接続可能なUSB Port一覧を取得する
///
/// # Example
/// ```
/// let device = ardeck::device::available_list();
/// ```
pub fn available_list() -> Vec<DeviceInfo> {
    serialport::available_ports()
        .unwrap_or(Vec::new())
        .into_iter()
        .filter_map(|port| match &port.port_type {
            SerialPortType::UsbPort(e) => {
                let port_info = PortInfo::from(e.clone());
                let device_id = make_device_id(&port_info);
                Some(DeviceInfo {
                    port_name: port.port_name.clone(),
                    usb_port_info: port_info,
                    device_id: device_id,
                })
            }
            _ => None,
        })
        .collect()
}
