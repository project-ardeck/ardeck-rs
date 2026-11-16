use std::{
    borrow::Cow,
    sync::{
        Arc, Mutex,
        atomic::AtomicBool,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serialport::{SerialPort, SerialPortType};

static CMD_REQUEST_ALL_SWITCH_INFO: u8 = 0x01;
static CMD_REQUEST_START_AUTO_MODE: u8 = 0x10;
static CMD_REQUEST_STOP_AUTO_MODE: u8 = 0x1F;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Serialport error: {0}")]
    Serialport(#[from] serialport::Error),

    #[error("Std io error: {0}")]
    StdIo(#[from] std::io::Error),

    #[error("Failed to parse the received data.")]
    ParseError,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SwitchKind {
    /// デジタルスイッチ。タクトスイッチ、トグルスイッチなど。
    #[default]
    Digital,
    /// アナログスイッチ。ポテンショメータ、ジョイスティックなど。
    Analog,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SwitchInfo {
    kind: SwitchKind,
    /// スイッチが接続されているピン番号。
    pin: u8,
    /// スイッチの状態を表す値。
    state: u16,
    /// データの受信時刻を表すタイムスタンプ
    timestamp: Timestamp,
}

impl SwitchInfo {
    pub fn kind(&self) -> SwitchKind {
        self.kind
    }

    pub fn pin(&self) -> u8 {
        self.pin
    }

    pub fn state(&self) -> u16 {
        self.state
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}

/// USBポートの情報
#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    vid: u16,
    pid: u16,
    serial_number: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
}

impl PortInfo {
    pub fn vid(&self) -> u16 {
        self.vid
    }

    pub fn pid(&self) -> u16 {
        self.pid
    }

    pub fn serial_number(&self) -> Option<&String> {
        self.serial_number.as_ref()
    }

    pub fn manufacturer(&self) -> Option<&String> {
        self.manufacturer.as_ref()
    }

    pub fn product(&self) -> Option<&String> {
        self.product.as_ref()
    }
}

impl From<serialport::UsbPortInfo> for PortInfo {
    fn from(value: serialport::UsbPortInfo) -> Self {
        PortInfo {
            vid: value.vid,
            pid: value.pid,
            serial_number: value.serial_number,
            manufacturer: value.manufacturer,
            product: value.product,
        }
    }
}

/// コンピューターに接続されて利用可能なシリアルポートデバイスの情報
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// ポート名
    port_name: String,
    /// 取得できたポート情報
    port_info: PortInfo,
    /// ポート情報から生成されたデバイスID
    device_id: String,
}

impl DeviceInfo {
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    pub fn port_info(&self) -> &PortInfo {
        &self.port_info
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

/// デバイス一覧の実装
#[allow(dead_code)]
trait DeviceInfosFilterTrait {
    /// デバイス一覧のうち、arduinoのベンダーコードを持つデバイスだけを抽出する。
    /// # Example
    /// ```
    /// let device = ardeck::device::available_list().arduino_only();
    /// ```
    fn arduino_only(self) -> Vec<DeviceInfo>;
}

impl DeviceInfosFilterTrait for Vec<DeviceInfo> {
    fn arduino_only(self) -> Vec<DeviceInfo> {
        self.into_iter()
            // 9025: Arduino LA のベンダーID
            .filter(|port| port.port_info.vid == 9025)
            .collect()
    }
}

/// ボタンの押下やアナログスイッチの操作をトリガーにし、キーボード入力等のアクションを発生させるための設定
///
/// ```json
/// [
///     {
///         switch_type: "Digital",
///         switch_id: 0,
///         plugin_id: "Keyboard",
///         action_id: "D"
///     },
///     ...
/// ]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileItem {
    switch_type: SwitchKind,
    switch_id: usize,
    plugin_id: String,
    action_id: String,
}

impl ProfileItem {
    pub fn switch_type(&self) -> SwitchKind {
        self.switch_type
    }

    pub fn switch_id(&self) -> usize {
        self.switch_id
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// プロファイルフォーマットバージョン: 1
    version: usize,
    /// プロファイル名
    profile_name: String,
    /// スイッチとアクションの紐づけ設定の配列
    maps: Vec<ProfileItem>,
}

impl Profile {
    pub fn version(&self) -> usize {
        self.version
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    pub fn maps(&self) -> &[ProfileItem] {
        &self.maps
    }
}

fn calc_checksum(bytes: impl AsRef<[u8]>) -> u8 {
    let bytes = bytes.as_ref();

    let mut sum: u8 = 0;

    for &byte in bytes {
        sum = sum.wrapping_add(byte);
    }

    sum
}

/// COBS形式のデータを生バイト値に変換する。
///
/// 正しく変換できなかった場合、[`None`]を返す。
fn dec_cobs(cobs_bytes: impl AsRef<[u8]>) -> Option<Vec<u8>> {
    let mut cobs_bytes = cobs_bytes.as_ref().to_vec();
    if *cobs_bytes.last()? != 0 {
        return None;
    }

    let mut i = 0;
    loop {
        let i_val = *cobs_bytes.get(i)?;

        cobs_bytes[i] = 0;

        if i_val == 0 {
            break;
        } else {
            i += i_val as usize;
        }
    }

    Some(cobs_bytes[1..cobs_bytes.len() - 1].to_vec())
}

fn raw_to_switch_info(bytes: impl AsRef<[u8]>) -> Option<SwitchInfo> {
    let bytes = bytes.as_ref().to_vec();

    #[cfg(not(test))]
    let timestamp = Timestamp::now();

    #[cfg(test)]
    let timestamp = Timestamp::default();

    // switch kind
    match bytes.get(0)? & 0x80 {
        // Digital Switch
        0 => {
            if bytes.len() == 1 {
                Some(SwitchInfo {
                    kind: SwitchKind::Digital,
                    pin: (bytes[0] & 0b01111110) >> 1,
                    state: (bytes[0] & 1) as u16,
                    timestamp,
                })
            } else {
                None
            }
        }
        // Analog Switch
        1 => {
            if bytes.len() == 2 {
                Some(SwitchInfo {
                    kind: SwitchKind::Analog,
                    pin: (bytes[0] & 0b01111100) >> 2,
                    state: ((bytes[0] as u16 & 0b11) << 8) | bytes[1] as u16,
                    timestamp,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

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
        .filter_map(|port| match port.port_type {
            SerialPortType::UsbPort(e) => {
                let port_info: PortInfo = e.into();
                let device_id = make_device_id(&port_info);
                Some(DeviceInfo {
                    port_name: port.port_name.clone(),
                    port_info,
                    device_id,
                })
            }
            _ => None,
        })
        .collect()
}

pub struct CobsReceiver {
    buffer: Vec<u8>,
}

impl CobsReceiver {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        for &byte in data {
            self.buffer.push(byte);
            if byte == 0x00 {
                if self.buffer.is_empty() {
                    // 空フレーム
                    continue;
                }

                // https://doc.rust-lang.org/std/mem/fn.take.html
                // 所有権ではなくて値そのものを移動してくるイメージ？移動された元の変数は空になる。
                let encoded = std::mem::take(&mut self.buffer);

                println!("encoded: {:?}", encoded);

                match dec_cobs(&encoded) {
                    Some(decoded) => {
                        if decoded.len() < 1 {
                            // checksumが存在しない
                            continue;
                        }

                        let payload_len = decoded.len() - 1;
                        let payload = &decoded[..payload_len];
                        let received_checksum = decoded[payload_len];

                        let calculated_checksum = calc_checksum(payload);

                        if received_checksum == calculated_checksum {
                            frames.push(payload.to_vec());
                        } else {
                            // checksum不一致
                            // ログ等
                        }
                    }
                    None => {
                        // 不正なCOBSフレーム
                        // ログ等
                    }
                }
            }
        }

        frames
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WorkerStatus {
    worker_write: bool,
    worker_read: bool,
    worker_action: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ControlMessage {
    /// すべてのスイッチの情報を送信するように要求する
    RequestAllSwitchInfo,
    /// 今後、Ardeckから自動的にスイッチ情報を送るように要求する
    RequestStartAutoMode,
    /// 自動送信を停止するように要求する
    RequestStopAutoMode,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("{0}")]
    CommunicationError(#[from] serialport::Error),
}

#[derive(Debug, Clone)]
pub struct ConnectionOption<'a> {
    port_name: Cow<'a, str>,
    baud_rate: u32,
    auto_reconnect: bool,
    auto_recv_mode: bool,
    // reconnect_interval: Duration,
    // read_timeout: Duration,
}

impl<'a> ConnectionOption<'a> {
    pub fn port_name(mut self, path: impl Into<std::borrow::Cow<'a, str>>) -> Self {
        self.port_name = path.into();
        self
    }

    pub fn baud_rate(mut self, rate: u32) -> Self {
        self.baud_rate = rate;
        self
    }

    pub fn auto_reconnect(mut self, opt: bool) -> Self {
        self.auto_reconnect = opt;
        self
    }

    pub fn auto_recv_mode(mut self, opt: bool) -> Self {
        self.auto_recv_mode = opt;
        self
    }
}

impl<'a> Default for ConnectionOption<'a> {
    fn default() -> Self {
        Self {
            port_name: "".into(),
            baud_rate: 9600,
            auto_reconnect: true,
            auto_recv_mode: false,
        }
    }
}

/// ardeckデバイスへの接続とデータ送受信
///
/// # Examples
/// ```
/// let connection = Connection::new("COM3", 9600).unwrap();
///
/// let rx = connection.action_receiver();
/// let rx = rx.lock().unwrap();
/// let exit_flag = connection.exit_flag();
///
/// loop {
///     if exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
///         break;
///     }
///     if let Ok(switch_info) = rx.recv() {
///         println!("{:?}", switch_info);
///     }
/// }
/// ```
pub struct Connection {
    worker_status: Arc<Mutex<WorkerStatus>>,
    exit_flag: Arc<AtomicBool>,
    control_tx: mpsc::Sender<ControlMessage>,
    action_rx: Arc<Mutex<mpsc::Receiver<Vec<SwitchInfo>>>>,
    is_auto_reconnect: Arc<AtomicBool>,
    is_auto_recv_mode: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Connection {
    pub fn new<'a>(option: ConnectionOption) -> Result<Self, ConnectionError> {
        // 全スレッド共有で処理状況のステータス保管用
        let worker_status = Arc::new(Mutex::new(WorkerStatus::default()));
        let exit_flag = Arc::new(AtomicBool::new(false));
        let port_name = option.port_name.to_string();
        let baud_rate = option.baud_rate;
        let is_auto_reconnect = Arc::new(AtomicBool::new(option.auto_reconnect));
        let is_auto_recv_mode = Arc::new(AtomicBool::new(option.auto_recv_mode));

        // log::info!("Connected to {:?}.", path);

        // 外から送信用への橋渡し
        let (control_tx, control_rx) = std::sync::mpsc::channel::<ControlMessage>();
        // 受信から処理への橋渡し
        let (action_tx, action_rx) = std::sync::mpsc::channel::<Vec<SwitchInfo>>();

        let worker_exit_flag = Arc::clone(&exit_flag);
        let worker_auto_reconnect = Arc::clone(&is_auto_reconnect);
        let worker_auto_recv_mode = Arc::clone(&is_auto_recv_mode);
        let worker = thread::spawn(move || {
            run_worker(
                port_name,
                baud_rate,
                worker_auto_reconnect,
                Duration::from_millis(100), // FIXME
                Duration::from_millis(100), // FIXME
                worker_auto_recv_mode,
                worker_exit_flag,
                control_rx,
                action_tx,
            )
        });

        let action_rx = Arc::new(Mutex::new(action_rx));

        Ok(Self {
            worker_status,
            exit_flag,
            control_tx,
            action_rx,
            is_auto_reconnect,
            is_auto_recv_mode,
            worker: Some(worker),
        })
    }

    pub fn action_receiver(&self) -> Arc<Mutex<mpsc::Receiver<Vec<SwitchInfo>>>> {
        self.action_rx.clone()
    }

    pub fn exit_flag(&self) -> Arc<AtomicBool> {
        self.exit_flag.clone()
    }

    pub fn conrol_sender(&self) -> &mpsc::Sender<ControlMessage> {
        &self.control_tx
    }

    /// Ardeckにすべてのスイッチ情報を送信するように要求します
    pub fn request_all_switch_info(&self) {
        self.control_tx
            .send(ControlMessage::RequestAllSwitchInfo)
            .ok();
    }

    /// Ardeckにスイッチの状態が変わったときに自動的に送信するように要求します
    pub fn request_start_auto_mode(&mut self) {
        if self
            .control_tx
            .send(ControlMessage::RequestStartAutoMode)
            .is_ok()
        {
            self.is_auto_recv_mode
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Ardeckにスイッチ情報の自動送信をやめるように要求します
    pub fn request_stop_auto_mode(&mut self) {
        if self
            .control_tx
            .send(ControlMessage::RequestStopAutoMode)
            .is_ok()
        {
            self.is_auto_recv_mode
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.request_stop_auto_mode();
        self.exit_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn run_worker(
    port_name: String,
    baud_rate: u32,
    auto_reconnect: Arc<AtomicBool>,
    reconnect_interval: Duration,
    read_timeout: Duration,
    auto_recv_mode: Arc<AtomicBool>,
    exit_flag: Arc<AtomicBool>,
    control_rx: Receiver<ControlMessage>,
    action_tx: Sender<Vec<SwitchInfo>>,
) {
    let mut buf = [0u8; 8];

    loop {
        if exit_flag.load(std::sync::atomic::Ordering::Acquire) {
            break;
        };

        let mut serialport = match serialport::new(&port_name, baud_rate)
            .timeout(read_timeout)
            .open()
        {
            Ok(port) => {
                log::info!("Connected to: {}", port_name);
                port
            }
            Err(e) => {
                log::error!("Failed to open serial port: {} / {}", port_name, e);

                if !auto_reconnect.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }

                // if

                continue;
            }
        };

        let mut cobs = CobsReceiver::new();

        let mut disconnect_reason: Option<String> = None;

        if auto_recv_mode.load(std::sync::atomic::Ordering::Acquire) {
            if let Err(e) = serialport
                .write_all(&[CMD_REQUEST_START_AUTO_MODE])
                .and_then(|_| serialport.flush())
            {
                continue;
            };
        }

        'recvloop: loop {
            if exit_flag.load(std::sync::atomic::Ordering::Acquire) {
                if let Err(e) = serialport
                    .write_all(&[CMD_REQUEST_STOP_AUTO_MODE])
                    .and_then(|_| serialport.flush())
                {
                    log::error!("Failed to stop auto mode: {}", e);
                }

                break 'recvloop;
            }

            loop {
                match control_rx.try_recv() {
                    Ok(cmd) => match cmd {
                        ControlMessage::RequestAllSwitchInfo => {
                            if let Err(e) = serialport
                                .write_all(&[CMD_REQUEST_ALL_SWITCH_INFO])
                                .and_then(|_| serialport.flush())
                            {
                                break 'recvloop;
                            };
                        }
                        ControlMessage::RequestStartAutoMode => {
                            if let Err(e) = serialport
                                .write_all(&[CMD_REQUEST_START_AUTO_MODE])
                                .and_then(|_| serialport.flush())
                            {
                                break 'recvloop;
                            };
                        }
                        ControlMessage::RequestStopAutoMode => {
                            if let Err(e) = serialport
                                .write_all(&[CMD_REQUEST_STOP_AUTO_MODE])
                                .and_then(|_| serialport.flush())
                            {
                                break 'recvloop;
                            };
                        }
                    },
                    Err(mpsc::TryRecvError::Empty) => {
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        exit_flag.store(true, std::sync::atomic::Ordering::Release);
                        break 'recvloop;
                    }
                }
            }

            match serialport.read(&mut buf) {
                Ok(read_len) => {
                    let data = &buf[0..read_len];

                    let decoded = cobs.push(data);
                    let switch_infos: Vec<SwitchInfo> = decoded
                        .iter()
                        .filter_map(|data| raw_to_switch_info(data))
                        .collect();

                    if let Err(e) = action_tx.send(switch_infos) {
                        log::error!("Failed to send action informations: {}", e);

                        if let Err(e) = serialport
                            .write_all(&[CMD_REQUEST_STOP_AUTO_MODE])
                            .and_then(|_| serialport.flush())
                        {
                            log::error!("Failed to stop auto mode: {}", e);
                        }
                    };
                }
                Err(e) => match e.kind() {
                    std::io::ErrorKind::TimedOut => {}
                    _ => break 'recvloop,
                },
            }
        }

        if !auto_reconnect.load(std::sync::atomic::Ordering::Acquire) {
            break;
        } else {
            thread::sleep(reconnect_interval);
        }
    }
}

// fn wait_or_exit(exit_flag: &AtomicBool, duration: Duration) -> bool {
//     let deadline = Instant::now() + duration;

//     loop {
//         if exit_flag.load(Ordering::Acquire) {
//             return true;
//         }

//         let remaining = deadline.saturating_duration_since(Instant::now());

//         if remaining.is_zero() {
//             return false;
//         }

//         thread::sleep(remaining.min(Duration::from_millis(50)));
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec() {
        let mut receiver = CobsReceiver::new();

        let result = receiver.push(&[
            0x02, 0x01, 0x00, // 単一のデータ(Data1)
            0x04, 0x11, // 次のデータの断片(Data2)
        ]);

        assert_eq!(result, vec![vec![0x01],]);

        let result = receiver.push(&[
            0x22, 0x33, 0x00, // (Data2)
            0x02, 0xAA, 0x00, // (Data3)
        ]);

        assert_eq!(result, vec![vec![0x11, 0x22, 0x33], vec![0xAA],]);
    }
}
