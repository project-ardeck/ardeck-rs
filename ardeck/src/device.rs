pub mod dec;
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
pub struct DeviceInfo {
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
pub fn available_list() -> Vec<DeviceInfo> {
    serialport::available_ports()
        .unwrap_or(Vec::new())
        .into_iter()
        .filter_map(|port| match &port.port_type {
            SerialPortType::UsbPort(e) => Some(DeviceInfo {
                port_name: port.port_name.clone(),
                usb_port_info: e.clone(),
                device_id: make_device_id(&e),
            }),
            _ => None,
        })
        .collect()
}

/// デバイス一覧の実装
pub trait DeviceInfoList {
    fn arduino_only(self) -> Vec<DeviceInfo>;
}

impl DeviceInfoList for Vec<DeviceInfo> {
    /// デバイス一覧のうち、arduinoのベンダーコードを持つデバイスだけを抽出する
    /// # Example
    /// ```
    /// let device = ardeck::device::available_list().arduino_only();
    /// ```
    fn arduino_only(self) -> Vec<DeviceInfo> {
        self.into_iter()
            // 9025: Arduino LA のベンダーID
            .filter(|port| port.usb_port_info.vid == 9025)
            .collect()
    }
}

#[derive(Debug)]
enum SessionErrorKind {
    InitializationError(String),
}

#[derive(Debug, thiserror::Error)]
enum Error {
    // #[error("Session error")]
    // Session(#[from] SessionErrorKind),
    #[error("Serialport error: `{0}`")]
    Serialport(#[from] serialport::Error),
}

#[derive(Debug)]
enum SessionState {
    /// 待機状態
    Standby,
    /// 初回接続中、または再接続中
    Connecting,
    /// 接続済み
    Connected,
    /// 切断済み
    Disconnected,
    /// 通信中にエラーが発生
    Error(Error),
}

pub type ArdeckConnectionHandler = Box<dyn Fn(SessionState) + Send + Sync + 'static>;

/// Ardeckとの通信を制御したり、データを処理したりする
pub struct Session {
    device_id: String,
    /// シリアルポートの接続
    serialport: Option<Box<dyn SerialPort>>,
    port_name: String,
    baud_rate: u32,

    state: SessionState,
    handler: Option<ArdeckConnectionHandler>,
    // recv_seqence:
}

impl Session {
    fn new(device_id: String, port_name: String, baud_rate: u32) -> Self {
        Self {
            device_id,
            serialport: None,
            baud_rate,
            port_name,
            state: SessionState::Standby,
            handler: None,
        }
    }

    /// 指定した端末への通信を開始します。
    pub fn start() {}
}

// DRAFT:
// - コネクションインスタンスが生成されると接続先を記録したインスタンスが生成される
// - インスタンスが存在する間はシリアルポートが切断されても再接続を試みる
// - 初回接続時に未接続ならばリトライ・アクセス拒否ならば初期化失敗としてインスタンスを生成しない
