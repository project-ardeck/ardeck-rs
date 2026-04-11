pub mod decode;
pub mod switch;

use std::{
    default, fmt,
    sync::{Arc, mpsc},
    thread::sleep,
    time::Duration,
};

use serialport::{SerialPort, SerialPortType, UsbPortInfo};
use smol::lock::Mutex;

use crate::device::decode::Decoder;

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
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// ポート名
    pub port_name: String,
    /// 取得できたポート情報
    pub usb_port_info: UsbPortInfo,
    /// ポート情報から生成されたデバイスID
    pub device_id: String,
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
            SerialPortType::UsbPort(e) => Some(DeviceInfo {
                port_name: port.port_name.clone(),
                usb_port_info: e.clone(),
                device_id: make_device_id(&e),
            }),
            _ => None,
        })
        .collect()
}

// TODO: 名前変える
/// デバイス一覧の実装
trait DeviceInfoList {
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

#[derive(Debug, Default)]
enum SessionState {
    /// 初回接続中、または再接続中
    Connecting,
    /// 接続済み
    Connected,
    /// 切断済み
    #[default]
    Disconnected,
    /// 通信中にエラーが発生
    Error(Error),
}

pub type ArdeckConnectionHandler = Box<dyn Fn(SessionState) + Send + Sync + 'static>;

/// セッションを作成する前に設定をおこないます。
#[derive(Debug, Clone)]
pub struct SessionBuilder {
    /// デバイス情報
    device_info: DeviceInfo,
    /// 接続の際に試行する最大回数
    connect_attempt_limit: u16,
    /// 失敗した後の次の試行までの待機時間
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

    /// 接続の際に試行する最大回数
    pub fn connect_attempt_limit(mut self, connect_attempt_limit: u16) -> Self {
        self.connect_attempt_limit = connect_attempt_limit;
        self
    }

    /// 失敗した後の次の試行までの待機時間
    pub fn connect_retry_interval(mut self, connect_retry_interval: Duration) -> Self {
        self.connect_retry_interval = connect_retry_interval;
        self
    }

    pub fn build(self) -> Session {
        Session::new(self)
    }
}

pub struct Session {
    cmd_tx: Option<mpsc::Sender<SessionMessage>>,
    // 接続中のデバイス情報
    device_info: DeviceInfo,
    /// 接続状況
    state: SessionState,
    /// ハンドラー
    handler: Arc<Mutex<Option<ArdeckConnectionHandler>>>,
    /// 接続試行時の試行回数の最大値 0の時は制限を設けない
    connect_attempt_limit: u16,
    /// 失敗した後の次の試行までの待機時間
    connect_retry_interval: Duration,
}

impl Session {
    pub fn new(builder: SessionBuilder) -> Self {
        log::info!("Session created: {}", builder.device_info.port_name);

        Self {
            cmd_tx: None,
            device_info: builder.device_info,
            state: SessionState::default(),
            handler: Arc::new(Mutex::new(None)),
            connect_attempt_limit: builder.connect_attempt_limit,
            connect_retry_interval: builder.connect_retry_interval,
        }
    }

    pub fn start(&mut self) {
        // 必要なものをクローンする
        let device_info = self.device_info.clone();
        let handler = self.handler.clone();
        let connect_attempt_limit = self.connect_attempt_limit;
        let connect_retry_interval = self.connect_retry_interval.clone();
        let (msg_tx, msg_rx) = mpsc::channel::<SessionMessage>();
        self.cmd_tx = Some(msg_tx);
        smol::spawn(async move {
            log::info!("daemon~!");
            'threadloop: loop {
                if let Ok(e) = msg_rx.try_recv() {
                    // TODO: 受け取り部分を1か所にまとめる
                    match e {
                        SessionMessage::Drop => break,
                    }
                }

                // TODO: もし接続が切れたらthreadloopをcontinueする
                let mut port = match serialport::new(&device_info.port_name, 9600).open() {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("{}", e.to_string());
                        smol::Timer::after(Duration::from_millis(1000)).await;
                        continue 'threadloop;
                    }
                };

                let mut decoder = Decoder::new();

                // readloop
                loop {
                    if let Ok(e) = msg_rx.try_recv() {
                        match e {
                            SessionMessage::Drop => break 'threadloop,
                        }
                    }

                    let mut buf: [u8; 16] = [0; 16];

                    match port.read(&mut buf) {
                        Ok(_) => {
                            log::debug!("received: {:?}", buf);
                            decoder.receive(&buf);
                            if let Some(data) = decoder.process_buffer() {
                                log::debug!("Received data!!! {:?}", data);
                            }
                            // 呼び出し
                            //
                        }
                        Err(e) => {
                            continue 'threadloop;
                        }
                    };
                }
            }
        })
        .detach();
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(cmd_tx) = &self.cmd_tx {
            log::debug!("Dropped: {}", self.device_info.port_name);
            cmd_tx.send(SessionMessage::Drop).unwrap();
        }
    }
}

/// SessionがDaemonに送信するメッセージ
enum SessionMessage {
    Drop,
}

// DRAFT:
// - コネクションインスタンスが生成されると接続先を記録したインスタンスが生成される
// - インスタンスが存在する間はシリアルポートが切断されても再接続を試みる
// - 初回接続時に未接続ならばリトライ・アクセス拒否ならば初期化失敗としてインスタンスを生成しない
