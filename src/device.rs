pub mod switch;

use std::fmt;

use serialport::{SerialPort, SerialPortType, UsbPortInfo};

/// デバイスのハードウェア固有番号を使用して、識別番号を作成する
fn make_device_id(port_info: &UsbPortInfo) -> String {
    if let Some(serial_number) = &port_info.serial_number {
        format!(
            "{:04X}-{:04X}-{}",
            port_info.vid, port_info.pid, serial_number
        )
    } else {
        format!("{:04X}-{:04X}", port_info.vid, port_info.pid)
    }
}

/// コンピューターに接続されて利用可能なシリアルポートデバイスの情報
pub struct AvailableDeviceInfo {
    /// ポート名
    pub port_name: String,
    /// 取得できたポート情報
    pub usb_port_info: UsbPortInfo,
    /// ポート情報から生成されたデバイスID
    pub device_id: String,
}

/// 接続可能なUSB Port一覧を取得する
/// # Example
/// ```
/// let device = ardeck::device::available_list();
/// ```
pub fn available_list() -> Vec<AvailableDeviceInfo> {
    serialport::available_ports()
        .unwrap_or(Vec::new())
        .into_iter()
        .filter_map(|port| match &port.port_type {
            SerialPortType::UsbPort(e) => Some(AvailableDeviceInfo {
                port_name: port.port_name.clone(),
                usb_port_info: e.clone(),
                device_id: make_device_id(&e),
            }),
            _ => None,
        })
        .collect()
}

/// デバイス一覧の実装
pub trait AvailableDeviceInfoList {
    fn arduino_only(self) -> Vec<AvailableDeviceInfo>;
}

impl AvailableDeviceInfoList for Vec<AvailableDeviceInfo> {
    /// デバイス一覧のうち、arduinoのベンダーコードを持つデバイスだけを抽出する
    /// # Example
    /// ```
    /// let device = ardeck::device::available_list().arduino_only();
    /// ```
    fn arduino_only(self) -> Vec<AvailableDeviceInfo> {
        self.into_iter()
            // 9025: Arduino LA のベンダーID
            .filter(|port| port.usb_port_info.vid == 9025)
            .collect()
    }
}

#[derive(Debug)]
enum ConnectionErrorKind {
    /// 初期化に失敗した。シリアルポートへのアクセスに失敗した可能性が高い。
    InitializationFailed,
    /// 通信中に接続を失った。
    ConnectionLost,
    // NotConnected,
    // SerialPort(serialport::Error),
}

impl fmt::Display for ConnectionErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InitializationFailed => write!(f, "Initialization failed."),
            Self::ConnectionLost => write!(f, "Connection lost during communication."),
            // Self::NotConnected => write!(f, ""),
            // ConnectionErrorKind::SerialPort(e) => ,
        }
    }
}

/// コネクションのハンドラー
#[derive(Debug)]
enum ConnectionHandlerType {
    /// 初回接続中、または再接続中
    Connecting,
    /// 接続済み
    Connected,
    /// 切断済み
    Disconnected,
    /// 通信中にエラーが発生
    Error(ConnectionErrorKind),
}

pub type ArdeckConnectionHandler = Box<dyn Fn(ConnectionHandlerType) + Send + Sync + 'static>;

/// Ardeckとの通信を制御したり、データを処理したりする
struct Connection {
    device_id: String,
    /// シリアルポートの接続
    serialport: Option<Box<dyn SerialPort>>,
    handler: Option<ArdeckConnectionHandler>,
}

// DRAFT:
// - コネクションインスタンスが生成されると接続先を記録したインスタンスが生成される
// - インスタンスが存在する間はシリアルポートが切断されても再接続を試みる
// - 初回接続時に未接続ならばリトライ・アクセス拒否ならば初期化失敗としてインスタンスを生成しない
impl Connection {
    /// 接続
    pub fn new(
        port_name: String,
        baud_rate: u32,
        device_id: String,
        handler: Option<ArdeckConnectionHandler>,
    ) -> Self {
        match serialport::new(port_name, baud_rate).open() {
            Ok(p) => Self {
                device_id,
                handler,
                serialport: Some(p),
            },
            Err(e) => device,
        };
    }
}

struct ConnectionBuilder {
    port_name: String,
    baud_rate: u32,
}
