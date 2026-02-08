pub mod dec;
pub mod switch;

use std::{fmt, thread::sleep, time::Duration};

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
    InitializationError,
    TimeOut,
}

impl fmt::Display for SessionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializationError => write!(f, "Failed initialization."),
            Self::TimeOut => write!(f, "Timeout."),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Session error: `{0}`")]
    Session(SessionErrorKind),
    #[error("Serialport error: `{0}`")]
    Serialport(#[from] serialport::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
enum SessionState {
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

/// セッションを作成する前に設定をおこないます。
pub struct SessionBuilder {
    device_info: DeviceInfo,
    connect_attempt_limit: u16,
    connect_retry_interval: Duration,
}

impl SessionBuilder {
    pub fn new(device_info: DeviceInfo) -> Self {
        Self {
            device_info,
            connect_attempt_limit: 0,
            connect_retry_interval: Duration::ZERO,
        }
    }

    pub fn connect_attempt_limit(mut self, connect_attempt_limit: u16) -> Self {
        self.connect_attempt_limit = connect_attempt_limit;
        self
    }

    pub fn connect_retry_interval(mut self, connect_retry_interval: Duration) -> Self {
        self.connect_retry_interval = connect_retry_interval;
        self
    }
}

/// Ardeckとの通信を制御したり、データを処理したりする
pub struct Session {
    /// 接続中のデバイス情報
    device_info: DeviceInfo,
    /// シリアルポートの接続
    serialport: Option<Box<dyn SerialPort>>,
    /// 接続状況
    state: SessionState,
    /// ハンドラー
    handler: Option<ArdeckConnectionHandler>,
    /// 接続試行時の試行回数の最大値 0の時は制限を設けない
    connect_attempt_limit: u16,
    connect_retry_interval: Duration,
    // recv_seqence:
}

impl Session {
    fn new(device_info: DeviceInfo) -> Self {
        Self {
            device_info,
            serialport: None,
            state: SessionState::Disconnected,
            handler: None,
            connect_attempt_limit: 0,
            connect_retry_interval: Duration::ZERO,
        }
    }

    /// 接続時の試行回数の最大値を設定する。0なら制限しない
    pub fn set_connect_attempt_limit(mut self, connect_attempt_limit: u16) -> Self {
        self.connect_attempt_limit = connect_attempt_limit;
        self
    }

    /// 接続時の試行回数の最大値
    pub fn connect_attempt_limit(&self) -> u16 {
        self.connect_attempt_limit
    }

    /// 接続時の再試行までの待機時間を設定する
    pub fn set_connect_retry_interval(mut self, connect_retry_interval: Duration) -> Self {
        self.connect_retry_interval = connect_retry_interval;
        self
    }

    /// 接続時の再試行までの待機時間
    pub fn connect_retry_interval(&self) -> Duration {
        self.connect_retry_interval
    }

    pub fn connect(&self) -> Result<Box<dyn SerialPort>> {
        let mut tryed: u16 = 0;
        loop {
            if let Some(port) = self.try_connect() {
                return Ok(port);
            }

            if self.connect_attempt_limit != 0 {
                tryed += 1;
                if tryed <= self.connect_attempt_limit() {
                    return Err(Error::Session(SessionErrorKind::TimeOut));
                }
            }

            sleep(self.connect_retry_interval);
        }
    }

    /// 接続試行
    ///
    /// 接続を試行します。成功すれば[`SerialPort`]を返し、失敗すれば[`Noneを返します`]
    fn try_connect(&self) -> Option<Box<dyn SerialPort>> {
        match serialport::new(&self.device_info.port_name, 9600).open() {
            Ok(p) => Some(p),
            Err(_e) => None,
        }
    }
}

// DRAFT:
// - コネクションインスタンスが生成されると接続先を記録したインスタンスが生成される
// - インスタンスが存在する間はシリアルポートが切断されても再接続を試みる
// - 初回接続時に未接続ならばリトライ・アクセス拒否ならば初期化失敗としてインスタンスを生成しない
